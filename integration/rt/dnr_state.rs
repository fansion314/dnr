//! Application preferences, separate from disposable caches and package versions.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, OnceLock},
    time::{Duration, Instant},
};

const DEBOUNCE: Duration = Duration::from_secs(5);
static MANAGER: OnceLock<Arc<Manager>> = OnceLock::new();

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Document {
    version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    zoom_factor: Option<f64>,
    #[serde(default)]
    windows: BTreeMap<String, [i32; 2]>,
}

#[derive(Default)]
struct Dirty {
    zoom: Option<f64>,
    windows: BTreeMap<String, [i32; 2]>,
}
impl Dirty {
    fn is_empty(&self) -> bool {
        self.zoom.is_none() && self.windows.is_empty()
    }
}

struct State {
    document: Document,
    dirty: Dirty,
    enabled: [bool; 2],
    zoom_changed: bool,
    deadline: Option<Instant>,
}
struct Manager {
    path: PathBuf,
    state: Mutex<State>,
    wake: Condvar,
    writer_started: OnceLock<()>,
}

fn read(path: &Path) -> io::Result<Document> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(Document {
                version: 1,
                ..Default::default()
            });
        }
        Err(error) => return Err(error),
    };
    let mut document: Document = serde_json::from_slice(&bytes)?;
    if document.version != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsupported application state version",
        ));
    }
    if document
        .zoom_factor
        .is_some_and(|v| crate::dnr_zoom_config::validate(v).is_err())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid saved application zoom",
        ));
    }
    document
        .windows
        .retain(|key, size| !key.is_empty() && size[0] > 0 && size[1] > 0);
    Ok(document)
}

impl Manager {
    fn load(path: PathBuf, legacy_zoom: Option<f64>) -> Arc<Self> {
        let mut document = read(&path).unwrap_or_else(|e| {
            eprintln!("dnr: cannot read application state: {e}; using defaults");
            Document {
                version: 1,
                ..Default::default()
            }
        });
        if document.zoom_factor.is_none() {
            document.zoom_factor = legacy_zoom;
        }
        Arc::new(Self {
            path,
            state: Mutex::new(State {
                document,
                dirty: Dirty::default(),
                enabled: [true, true],
                zoom_changed: false,
                deadline: None,
            }),
            wake: Condvar::new(),
            writer_started: OnceLock::new(),
        })
    }
    fn schedule(self: &Arc<Self>, state: &mut State) {
        state.deadline = Some(Instant::now() + DEBOUNCE);
        self.writer_started.get_or_init(|| {
            let manager = self.clone();
            std::thread::Builder::new()
                .name("dnr-app-state".into())
                .spawn(move || manager.run())
                .expect("application state writer");
        });
        self.wake.notify_one();
    }
    fn run(&self) {
        let mut state = self.state.lock().unwrap();
        loop {
            match state.deadline {
                None => state = self.wake.wait(state).unwrap(),
                Some(deadline) if Instant::now() < deadline => {
                    let wait = deadline.saturating_duration_since(Instant::now());
                    state = self.wake.wait_timeout(state, wait).unwrap().0;
                }
                Some(_) => self.flush_locked(&mut state),
            }
        }
    }
    fn flush_locked(&self, state: &mut State) {
        state.deadline = None;
        if state.dirty.is_empty() {
            return;
        }
        if let Err(error) = merge_write(&self.path, &state.dirty, state.document.zoom_factor) {
            eprintln!("dnr: cannot save application state: {error}");
            // Keep dirty fields for the exit barrier or the next edit; do not spin on failure.
            return;
        }
        state.dirty = Dirty::default();
    }
}

fn merge_write(path: &Path, dirty: &Dirty, legacy_zoom: Option<f64>) -> io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let parent = path.parent().unwrap();
    fs::create_dir_all(parent)?;
    let lock_path = path.with_extension("lock");
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(lock_path)?;
    lock.lock()?;
    let mut document = read(path).unwrap_or_else(|_| Document {
        version: 1,
        ..Default::default()
    });
    if let Some(zoom) = dirty.zoom {
        document.zoom_factor = Some(zoom);
    } else if document.zoom_factor.is_none() {
        document.zoom_factor = legacy_zoom;
    }
    for (key, size) in &dirty.windows {
        document.windows.insert(key.clone(), *size);
    }
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temp, &document)?;
    temp.write_all(b"\n")?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

pub fn load_application(storage_id: &str) -> f64 {
    let Ok(config) = crate::dnr_zoom_config::config_path() else {
        return 1.0;
    };
    let root = config.parent().unwrap();
    let legacy = root.join("app-zoom").join(format!("{storage_id}.json"));
    let legacy = if legacy.exists() {
        match crate::dnr_zoom_config::read(&legacy) {
            Ok(factor) => Some(factor),
            Err(error) => {
                eprintln!("dnr: cannot read application zoom: {error}; using 1.0");
                None
            }
        }
    } else {
        None
    };
    let manager = Manager::load(
        root.join("app-state").join(format!("{storage_id}.json")),
        legacy,
    );
    let factor = manager
        .state
        .lock()
        .unwrap()
        .document
        .zoom_factor
        .unwrap_or(1.0);
    let _ = MANAGER.set(manager);
    factor
}

pub fn settings() -> [bool; 2] {
    MANAGER
        .get()
        .map(|m| m.state.lock().unwrap().enabled)
        .unwrap_or([true, true])
}

/// True asks the controller to undo an untouched startup zoom restoration.
pub fn set_settings(zoom: bool, size: bool) -> bool {
    let Some(manager) = MANAGER.get() else {
        return false;
    };
    let mut state = manager.state.lock().unwrap();
    let reset = state.enabled[0] && !zoom && !state.zoom_changed;
    state.enabled = [zoom, size];
    if !zoom {
        state.dirty.zoom = None;
    }
    if !size {
        state.dirty.windows.clear();
    }
    if state.dirty.is_empty() {
        state.deadline = None;
    }
    manager.wake.notify_one();
    reset
}

pub fn zoom_changed(factor: f64) {
    let Some(manager) = MANAGER.get() else {
        return;
    };
    let mut state = manager.state.lock().unwrap();
    state.zoom_changed = true;
    if !state.enabled[0] {
        return;
    }
    if state.document.zoom_factor == Some(factor) {
        return;
    }
    state.document.zoom_factor = Some(factor);
    state.dirty.zoom = Some(factor);
    manager.schedule(&mut state);
}

pub fn size(key: &str) -> Option<[i32; 2]> {
    let state = MANAGER.get()?.state.lock().unwrap();
    state.enabled[1]
        .then(|| state.document.windows.get(key).copied())
        .flatten()
}

pub fn record_size(key: &str, width: i32, height: i32) {
    if key.is_empty() || width <= 0 || height <= 0 {
        return;
    }
    let Some(manager) = MANAGER.get() else {
        return;
    };
    let mut state = manager.state.lock().unwrap();
    let size = [width, height];
    if !state.enabled[1] || state.document.windows.get(key) == Some(&size) {
        return;
    }
    state.document.windows.insert(key.into(), size);
    state.dirty.windows.insert(key.into(), size);
    manager.schedule(&mut state);
}

pub fn flush() {
    if let Some(manager) = MANAGER.get() {
        manager.flush_locked(&mut manager.state.lock().unwrap());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn merges_only_dirty_fields_and_imports_legacy_zoom() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut size = Dirty::default();
        size.windows.insert("main".into(), [900, 700]);
        merge_write(&path, &size, Some(1.2)).unwrap();
        merge_write(
            &path,
            &Dirty {
                zoom: Some(1.5),
                ..Default::default()
            },
            None,
        )
        .unwrap();
        size.windows.insert("settings".into(), [600, 400]);
        merge_write(&path, &size, Some(1.2)).unwrap();
        let state = read(&path).unwrap();
        assert_eq!(state.zoom_factor, Some(1.5));
        assert_eq!(state.windows.len(), 2);
    }
    #[test]
    fn flush_coalesces_updates_and_noop_does_not_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let manager = Manager::load(path.clone(), Some(1.1));
        let mut state = manager.state.lock().unwrap();
        manager.flush_locked(&mut state);
        assert!(!path.exists());
        for factor in [1.2, 1.5, 1.75] {
            state.dirty.zoom = Some(factor);
        }
        manager.flush_locked(&mut state);
        assert_eq!(read(&path).unwrap().zoom_factor, Some(1.75));
        assert!(state.dirty.is_empty());
    }
}
