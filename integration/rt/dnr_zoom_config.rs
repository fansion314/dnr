//! User-wide content zoom. This module must not initialize the runtime or GUI.
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Config {
    version: u32,
    zoom_factor: f64,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

pub fn validate(factor: f64) -> io::Result<f64> {
    if factor.is_finite() && (0.5..=2.0).contains(&factor) {
        Ok(factor)
    } else {
        Err(invalid(
            "zoom factor must be a finite number between 0.5 and 2.0",
        ))
    }
}

pub fn config_path() -> io::Result<PathBuf> {
    config_path_from(cfg!(target_os = "macos"), |name| std::env::var_os(name))
}

fn config_path_from(
    macos: bool,
    env: impl Fn(&str) -> Option<std::ffi::OsString>,
) -> io::Result<PathBuf> {
    let home = || {
        env("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| invalid("HOME is unset; set DNR_CONFIG_DIR"))
    };
    let base = if let Some(path) = env("DNR_CONFIG_DIR") {
        PathBuf::from(path)
    } else if macos {
        home()?.join("Library/Application Support/dnr")
    } else if let Some(path) = env("XDG_CONFIG_HOME").filter(|s| !s.is_empty()) {
        PathBuf::from(path).join("dnr")
    } else {
        home()?.join(".config/dnr")
    };
    if !base.is_absolute() {
        return Err(invalid(
            "dnr configuration directory must be an absolute path",
        ));
    }
    Ok(base.join("zoom.json"))
}

pub fn read(path: &Path) -> io::Result<f64> {
    let data = match std::fs::read(path) {
        Ok(data) => data,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(1.0),
        Err(error) => return Err(error),
    };
    let config: Config = serde_json::from_slice(&data).map_err(|e| invalid(e.to_string()))?;
    if config.version != 1 {
        return Err(invalid("unsupported zoom configuration version"));
    }
    validate(config.zoom_factor)
}

pub fn load_for_application() -> f64 {
    match config_path().and_then(|path| read(&path)) {
        Ok(factor) => factor,
        Err(error) => {
            eprintln!("dnr: cannot read zoom configuration: {error}; using 1.0");
            1.0
        }
    }
}

pub fn write(path: &Path, factor: f64) -> io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    validate(factor)?;
    let parent = path
        .parent()
        .ok_or_else(|| invalid("missing configuration directory"))?;
    std::fs::create_dir_all(parent)?;
    let (temporary, mut file) = loop {
        let temporary = parent.join(format!(
            ".zoom-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temporary) {
            Ok(file) => break (temporary, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let result = (|| {
        serde_json::to_writer_pretty(
            &mut file,
            &Config {
                version: 1,
                zoom_factor: factor,
            },
        )?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

pub fn command(args: &[String]) -> Result<(), String> {
    if args.len() == 2 && matches!(args[1].as_str(), "--help" | "-h") {
        println!(
            "dnr zoom\ndnr zoom set <factor: 0.5..2.0>\ndnr zoom reset\nChanges apply to newly started applications. Application zoom multiplies this factor."
        );
        return Ok(());
    }
    let factor = match args {
        [_] => None,
        [_, reset] if reset == "reset" => Some(1.0),
        [_, set, value] if set == "set" => Some(
            validate(value.parse().map_err(|_| "invalid zoom factor")?)
                .map_err(|e| e.to_string())?,
        ),
        _ => return Err("usage: dnr zoom [set <factor>|reset]".into()),
    };
    let path = config_path().map_err(|e| e.to_string())?;
    if let Some(factor) = factor {
        write(&path, factor).map_err(|e| format!("{}: {e}", path.display()))?;
        println!(
            "Global zoom: {factor} ({}%)\nConfig: {}\nRestart applications to apply this setting.",
            factor * 100.0,
            path.display()
        );
    } else {
        let factor = read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        println!(
            "Global zoom: {factor} ({}%)\nConfig: {}",
            factor * 100.0,
            path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_paths_and_overrides() {
        let env = |name: &str| match name {
            "HOME" => Some("/home/test".into()),
            _ => None,
        };
        assert_eq!(
            config_path_from(false, env).unwrap(),
            Path::new("/home/test/.config/dnr/zoom.json")
        );
        assert_eq!(
            config_path_from(true, env).unwrap(),
            Path::new("/home/test/Library/Application Support/dnr/zoom.json")
        );
        assert_eq!(
            config_path_from(false, |n| if n == "XDG_CONFIG_HOME" {
                Some("/settings".into())
            } else {
                env(n)
            })
            .unwrap(),
            Path::new("/settings/dnr/zoom.json")
        );
        assert_eq!(
            config_path_from(true, |n| if n == "DNR_CONFIG_DIR" {
                Some("/override".into())
            } else {
                env(n)
            })
            .unwrap(),
            Path::new("/override/zoom.json")
        );
        assert!(config_path_from(false, |_| Some("relative".into())).is_err());
    }

    #[test]
    fn read_write_reset_and_repair() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings/zoom.json");
        assert_eq!(read(&path).unwrap(), 1.0);
        assert!(!path.parent().unwrap().exists());
        write(&path, 1.25).unwrap();
        assert_eq!(read(&path).unwrap(), 1.25);
        for content in [
            "broken",
            r#"{"version":2,"zoomFactor":1}"#,
            r#"{"version":1,"zoomFactor":3}"#,
        ] {
            std::fs::write(&path, content).unwrap();
            assert!(read(&path).is_err());
        }
        write(&path, 1.0).unwrap();
        assert_eq!(read(&path).unwrap(), 1.0);
        for invalid in [f64::NAN, f64::INFINITY, 0.0, 0.49, 2.01] {
            assert!(write(&path, invalid).is_err());
            assert_eq!(read(&path).unwrap(), 1.0);
        }
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );
        let blocked = dir.path().join("file");
        std::fs::write(&blocked, "not a directory").unwrap();
        assert!(write(&blocked.join("zoom.json"), 1.2).is_err());
    }
}
