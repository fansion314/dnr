// DNR lifecycle: the caller thread owns the native event loop; JavaScript runs
// separately and asks for GUI initialization only when a Laufey API is used.
use std::ffi::CString;
use std::sync::{OnceLock, mpsc};
enum MainRequest {
    Activate(mpsc::SyncSender<Result<(), String>>),
    Finished(i32),
}
static MAIN: OnceLock<mpsc::Sender<MainRequest>> = OnceLock::new();
static READY: Mutex<Option<mpsc::SyncSender<Result<(), String>>>> = Mutex::new(None);
static INITIALIZING: Mutex<()> = Mutex::new(());
static LIVE: OnceLock<WefDesktopApi> = OnceLock::new();
static GUI_READY: tokio::sync::Notify = tokio::sync::Notify::const_new();

pub async fn pump_bindings() {
    while !laufey::is_initialized() {
        GUI_READY.notified().await;
    }
    laufey::run().await;
}

unsafe extern "C" {
    fn dnr_native_main(argc: i32, argv: *mut *mut std::ffi::c_char) -> i32;
    fn dnr_native_preflight(argc: i32, argv: *mut *mut std::ffi::c_char) -> i32;
}

fn ensure_backend() {
    let _lock = INITIALIZING.lock().unwrap();
    if laufey::is_initialized() {
        return;
    }
    let (tx, rx) = mpsc::sync_channel(1);
    MAIN.get().unwrap().send(MainRequest::Activate(tx)).unwrap();
    if let Err(error) = rx
        .recv()
        .unwrap_or_else(|_| Err("native backend exited during initialization".into()))
    {
        eprintln!("dnr: {error}");
        std::process::exit(1);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn laufey_runtime_init(api: *const laufey::LaufeyBackendApi) -> i32 {
    let result = unsafe { laufey::init_api(api) };
    if result == 0 {
        laufey::set_js_namespace("bindings");
        if let Some(live) = LIVE.get() {
            let tx = live.event_tx.clone();
            laufey::on_dock_reopen(move |has_visible_windows| {
                let _ = tx.try_send(deno_runtime::ops::desktop::DesktopEvent::DockReopen {
                    has_visible_windows,
                });
            });
        }
        GUI_READY.notify_one();
    }
    if let Some(reply) = READY.lock().unwrap().take() {
        let _ = reply.send(if result == 0 {
            Ok(())
        } else {
            Err(format!("Laufey ABI initialization failed ({result})"))
        });
    }
    result
}
#[unsafe(no_mangle)]
pub extern "C" fn laufey_runtime_start() -> i32 {
    // DNR already owns the runtime thread; don't start a second runtime.
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn laufey_runtime_shutdown() {}

pub fn has_live_objects() -> bool {
    LIVE.get().is_some_and(|api| {
        !api.open_windows.lock().unwrap().is_empty() || !api.trays.lock().unwrap().is_empty()
    })
}

pub async fn run_worker(
    worker: &mut deno_lib::worker::LibMainWorker,
) -> Result<i32, deno_core::error::AnyError> {
    use deno_runtime::deno_os::{WatcherExitHandle, WatcherExited};
    use std::future::{Future, poll_fn};
    use std::task::Poll;

    // Reuse Deno's isolate termination path so Deno.exit()/process.exit()
    // preserve the requested status without running GUI cleanup off-thread.
    let handle = worker.js_runtime().v8_isolate().thread_safe_handle();
    let state = worker.js_runtime().op_state();
    state.borrow_mut().put(WatcherExitHandle(handle));
    let result: Result<(), deno_core::error::AnyError> = {
        let mut run = std::pin::pin!(async {
            worker.execute_load_phase().await?;
            loop {
                worker.run_event_loop(false).await?;
                if !has_live_objects() {
                    if worker.dispatch_beforeunload_event()? {
                        continue;
                    }
                    if worker.dispatch_process_beforeexit_event()? {
                        continue;
                    }
                    if !has_live_objects() {
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            worker.dispatch_unload_event()?;
            worker.dispatch_process_exit_event()?;
            Ok(())
        });
        poll_fn(|cx| {
            let result = run.as_mut().poll(cx);
            // A native event listener runs in an async-op continuation. V8
            // termination there need not finish the event-loop future while
            // an HTTP server or another referenced task is still active.
            // Honor the exit request after every poll, including Pending.
            if state.borrow().has::<WatcherExited>() {
                Poll::Ready(Ok(()))
            } else {
                result
            }
        })
        .await
    };
    result?;
    Ok(worker.exit_code())
}

pub fn options() -> RunOptions {
    RunOptions {
        auto_serve: false,
        op_state_init: Some(Box::new(|state| {
            let (event_tx, event_rx) = denort::desktop::create_desktop_event_channel();
            let pending = denort::desktop::PendingBindResponses::new();
            let api = WefDesktopApi {
                event_tx: event_tx.0.clone(),
                pending_responses: pending.clone(),
                closed_windows: Arc::new(Mutex::new(HashSet::new())),
                open_windows: Arc::new(Mutex::new(HashSet::new())),
                trays: Arc::new(Mutex::new(HashMap::new())),
                notifications: Arc::new(Mutex::new(HashMap::new())),
                devtools_window: Mutex::new(None),
            };
            let live = WefDesktopApi {
                event_tx: api.event_tx.clone(),
                pending_responses: api.pending_responses.clone(),
                closed_windows: api.closed_windows.clone(),
                open_windows: api.open_windows.clone(),
                trays: api.trays.clone(),
                notifications: api.notifications.clone(),
                devtools_window: Mutex::new(None),
            };
            let _ = LIVE.set(live);
            denort::desktop::init_desktop_state(state, Box::new(api), None);
            state.put(event_rx);
            state.put(event_tx);
            state.put(pending);
            state.put(denort::desktop::InitialWindowId(Mutex::new(None)));
        })),
        ..Default::default()
    }
}

pub fn main(args: Vec<String>) {
    let mut native_args: Vec<CString> =
        std::env::args().map(|s| CString::new(s).unwrap()).collect();
    let mut pointers: Vec<_> = native_args
        .iter_mut()
        .map(|s| s.as_ptr().cast_mut())
        .collect();
    pointers.push(std::ptr::null_mut());
    let preflight =
        unsafe { dnr_native_preflight((pointers.len() - 1) as i32, pointers.as_mut_ptr()) };
    if preflight >= 0 {
        std::process::exit(preflight);
    }
    let data = match crate::dnr::application(args) {
        Ok(data) => data,
        Err(error) => {
            eprintln!("dnr: {error:#}");
            std::process::exit(1);
        }
    };
    // Environment changes are made before the runtime or native threads exist.
    unsafe {
        std::env::set_var("LAUFEY_RUNTIME_PATH", "dnr-static");
        if cfg!(target_os = "linux")
            && (std::env::var_os("WAYLAND_DISPLAY").is_some()
                || std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("wayland"))
        {
            std::env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1");
        }
    }
    let (tx, rx) = mpsc::channel();
    let _ = MAIN.set(tx.clone());
    laufey::set_lazy_initializer(ensure_backend);
    crate::init_logging(None, None);
    deno_runtime::deno_permissions::mark_standalone();
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    std::thread::Builder::new()
        .name("dnr-runtime".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            let runtime = deno_runtime::tokio_util::create_basic_runtime();
            let code = match runtime.block_on(crate::dnr::execute(data)) {
                Ok(code) => code,
                Err(error) => {
                    eprintln!("dnr: {error:#}");
                    1
                }
            };
            let _ = tx.send(MainRequest::Finished(code));
            // Return the native loop to the main thread before exiting. GTK /
            // WebKit exit handlers and CEF shutdown must run on that thread.
            if laufey::is_initialized() {
                laufey::quit();
            }
        })
        .expect("creating runtime thread");
    loop {
        match rx.recv() {
            Ok(MainRequest::Finished(code)) => std::process::exit(code),
            Ok(MainRequest::Activate(reply)) => {
                *READY.lock().unwrap() = Some(reply);
                // The backend sees only its executable name. Application args
                // belong exclusively to Deno; CEF helper args were handled above.
                let mut argv = [native_args[0].as_ptr().cast_mut(), std::ptr::null_mut()];
                let code = unsafe { dnr_native_main(1, argv.as_mut_ptr()) };
                if let Some(reply) = READY.lock().unwrap().take() {
                    let _ = reply.send(Err(format!("native backend failed ({code})")));
                }
                if code != 0 {
                    std::process::exit(code);
                }
            }
            Err(_) => std::process::exit(1),
        }
    }
}
