use scriptaro_core::{Action, AppSelector, Bounds, Script, WindowSelector};
use scriptaro_engine::{Engine, EngineError, RunOptions, RunStatus};
use scriptaro_platform::{
    BackendError, BackendResult, Capability, DesktopBackend, PendingScreenshot, WindowTarget,
    recording::{Operation, RecordingBackend},
};
use std::{
    cell::Cell,
    fs,
    path::PathBuf,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use tokio::time::{Instant, sleep};

const BOUNDS: Bounds = Bounds {
    x: -50.0,
    y: 20.0,
    width: 800.0,
    height: 600.0,
};
// A fixture payload, never presented as a real screenshot. Native PNG encoding
// has a separate synthetic-image test in the macOS backend.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfixture";
fn window() -> WindowSelector {
    WindowSelector {
        app: AppSelector::Name("Demo".into()),
        title: "Scratch".into(),
    }
}
fn arrange(timeout: u64) -> Action {
    Action::SetWindowBounds {
        window: window(),
        bounds: BOUNDS,
        timeout_ms: Some(timeout),
    }
}
fn capture(timeout: u64) -> Action {
    Action::Screenshot {
        path: "shot.png".into(),
        region: Some(BOUNDS),
        timeout_ms: Some(timeout),
    }
}
fn text() -> Action {
    Action::TypeText {
        profile: None,
        text: "x".into(),
        interval_ms: None,
    }
}
fn script(steps: Vec<Action>) -> Script {
    Script {
        version: 1,
        name: None,
        defaults: Default::default(),
        sections: vec![],
        steps,
    }
}
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "scriptaro-capture-tests-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn options(&self) -> RunOptions {
        RunOptions {
            base_dir: self.0.clone(),
            speed: 100.0,
            ..Default::default()
        }
    }
    fn empty(&self) -> bool {
        fs::read_dir(&self.0).unwrap().next().is_none()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
struct DropSignal(Rc<Cell<bool>>);
impl Drop for DropSignal {
    fn drop(&mut self) {
        self.0.set(true);
    }
}
struct Fixture {
    start: Instant,
    mutations: usize,
    queries: usize,
    captures: usize,
    typed: usize,
    missing_ms: u64,
    ready_ms: u64,
    capture_ms: u64,
    fail: bool,
    invalid_png: bool,
    conflict: Option<PathBuf>,
    dropped: Rc<Cell<bool>>,
    supported: bool,
}
impl Default for Fixture {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            mutations: 0,
            queries: 0,
            captures: 0,
            typed: 0,
            missing_ms: 40,
            ready_ms: 100,
            capture_ms: 80,
            fail: false,
            invalid_png: false,
            conflict: None,
            dropped: Rc::new(Cell::new(false)),
            supported: true,
        }
    }
}
impl DesktopBackend for Fixture {
    fn name(&self) -> &'static str {
        "fault injection; no desktop"
    }
    fn capabilities(&self) -> &'static [Capability] {
        if self.supported {
            &[
                Capability::Windows,
                Capability::WindowBounds,
                Capability::Screenshot,
                Capability::Keyboard,
            ]
        } else {
            &[Capability::Keyboard]
        }
    }
    fn set_window_bounds(
        &mut self,
        _: &WindowSelector,
        _: Bounds,
    ) -> BackendResult<Option<WindowTarget>> {
        if self.fail {
            return Err(BackendError::AmbiguousWindow);
        }
        if self.start.elapsed() < Duration::from_millis(self.missing_ms) {
            return Ok(None);
        }
        self.mutations += 1;
        Ok(Some(WindowTarget {
            app: AppSelector::Pid(42),
            id: 7,
        }))
    }
    fn window_bounds(&mut self, target: &WindowTarget) -> BackendResult<Bounds> {
        assert_eq!(target.id, 7);
        assert_eq!(target.app, AppSelector::Pid(42));
        self.queries += 1;
        Ok(Bounds {
            x: BOUNDS.x
                + if self.start.elapsed() >= Duration::from_millis(self.ready_ms) {
                    0.5
                } else {
                    30.0
                },
            ..BOUNDS
        })
    }
    fn screenshot(&mut self, _: Option<Bounds>) -> BackendResult<PendingScreenshot> {
        self.captures += 1;
        let ms = self.capture_ms;
        let fail = self.fail;
        let invalid = self.invalid_png;
        let conflict = self.conflict.clone();
        let guard = DropSignal(self.dropped.clone());
        Ok(Box::pin(async move {
            let _guard = guard;
            sleep(Duration::from_millis(ms)).await;
            if let Some(path) = conflict {
                fs::write(path, "external file").unwrap();
            }
            if fail {
                Err(BackendError::Native("capture refused".into()))
            } else {
                Ok(if invalid {
                    b"not png".to_vec()
                } else {
                    PNG.to_vec()
                })
            }
        }))
    }
    fn type_character(&mut self, _: char) -> BackendResult<()> {
        self.typed += 1;
        Ok(())
    }
}

#[tokio::test(start_paused = true)]
async fn bounds_poll_missing_then_dispatch_once_and_verify_retained_identity() {
    let mut backend = Fixture::default();
    Engine::new(
        &mut backend,
        RunOptions {
            speed: 100.0,
            ..Default::default()
        },
    )
    .run(&script(vec![arrange(200), text()]))
    .await
    .unwrap();
    assert_eq!(backend.mutations, 1);
    assert_eq!(backend.typed, 1);
    assert_eq!(backend.start.elapsed(), Duration::from_millis(100));
    assert_eq!(backend.queries, 4);
    for fail in [true, false] {
        let mut backend = Fixture {
            ready_ms: 1000,
            fail,
            ..Default::default()
        };
        let result = Engine::new(&mut backend, RunOptions::default())
            .run(&script(vec![arrange(150), text()]))
            .await;
        assert!(result.is_err());
        assert_eq!(backend.typed, 0);
        assert_eq!(backend.mutations, usize::from(!fail));
    }
}

#[tokio::test(start_paused = true)]
async fn bounds_cancel_and_pause_never_redispatch_or_type_after_expiry() {
    for cancel in [true, false] {
        let mut backend = Fixture {
            ready_ms: 1000,
            ..Default::default()
        };
        let engine = Engine::new(&mut backend, RunOptions::default());
        let controller = engine.controller();
        let sequence = script(vec![arrange(150), text()]);
        let (result, ()) = tokio::join!(engine.run(&sequence), async {
            sleep(Duration::from_millis(60)).await;
            if cancel {
                controller.cancel();
            } else {
                controller.pause();
                sleep(Duration::from_millis(200)).await;
                controller.resume();
            }
        });
        if cancel {
            assert_eq!(result.unwrap().status, RunStatus::Cancelled);
        } else {
            assert!(result.is_err());
        }
        assert_eq!(backend.mutations, 1);
        assert_eq!(backend.typed, 0);
    }
}

#[tokio::test(start_paused = true)]
async fn screenshot_publishes_bytes_beside_script_and_cleans_staging() {
    let root = Scratch::new();
    let mut backend = Fixture::default();
    Engine::new(&mut backend, root.options())
        .run(&script(vec![capture(200), text()]))
        .await
        .unwrap();
    assert_eq!(fs::read(root.0.join("shot.png")).unwrap(), PNG);
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1);
    assert_eq!(backend.captures, 1);
    assert_eq!(backend.typed, 1);
    assert_eq!(backend.start.elapsed(), Duration::from_millis(80));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(root.0.join("shot.png"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[tokio::test(start_paused = true)]
async fn screenshot_cancellation_timeout_errors_and_races_never_leave_partial_output() {
    for cause in [
        "cancel", "timeout", "pause", "native", "invalid", "conflict",
    ] {
        let root = Scratch::new();
        let mut backend = Fixture {
            capture_ms: if cause == "timeout" { 1000 } else { 80 },
            fail: cause == "native",
            invalid_png: cause == "invalid",
            conflict: (cause == "conflict").then(|| root.0.join("shot.png")),
            ..Default::default()
        };
        let engine = Engine::new(&mut backend, root.options());
        let controller = engine.controller();
        let sequence = script(vec![capture(150), text()]);
        let (result, ()) = tokio::join!(engine.run(&sequence), async {
            sleep(Duration::from_millis(40)).await;
            if cause == "cancel" {
                controller.cancel();
            }
            if cause == "pause" {
                controller.pause();
                sleep(Duration::from_millis(200)).await;
                controller.resume();
            }
        });
        if cause == "cancel" {
            assert_eq!(result.unwrap().status, RunStatus::Cancelled);
        } else {
            assert!(result.is_err(), "{cause}");
        }
        assert_eq!(backend.captures, 1);
        assert_eq!(backend.typed, 0);
        assert!(backend.dropped.get());
        if cause == "conflict" {
            assert_eq!(
                fs::read_to_string(root.0.join("shot.png")).unwrap(),
                "external file"
            );
            assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1);
        } else {
            assert!(root.empty(), "{cause}");
        }
    }
}

#[tokio::test(start_paused = true)]
async fn output_and_capability_preflight_stop_prior_effects_and_do_not_overwrite() {
    let root = Scratch::new();
    fs::write(root.0.join("shot.png"), "original").unwrap();
    for steps in [
        vec![text(), capture(200)],
        vec![
            text(),
            Action::Screenshot {
                path: "missing/shot.png".into(),
                region: None,
                timeout_ms: None,
            },
        ],
    ] {
        let mut backend = Fixture::default();
        assert!(matches!(
            Engine::new(&mut backend, root.options())
                .run(&script(steps))
                .await,
            Err(EngineError::Preflight(_))
        ));
        assert_eq!(backend.typed, 0);
        assert_eq!(backend.captures, 0);
    }
    assert_eq!(
        fs::read_to_string(root.0.join("shot.png")).unwrap(),
        "original"
    );
    fs::remove_file(root.0.join("shot.png")).unwrap();
    let mut backend = Fixture::default();
    assert!(matches!(
        Engine::new(&mut backend, root.options())
            .run(&script(vec![text(), capture(200), capture(200)]))
            .await,
        Err(EngineError::Preflight(_))
    ));
    assert_eq!(backend.typed, 0);
    assert!(root.empty());
    for action in [arrange(200), capture(200)] {
        let mut backend = Fixture {
            supported: false,
            ..Default::default()
        };
        assert!(matches!(
            Engine::new(&mut backend, root.options())
                .run(&script(vec![text(), action]))
                .await,
            Err(EngineError::Preflight(_))
        ));
        assert_eq!(backend.typed, 0);
    }
}

#[tokio::test(start_paused = true)]
async fn dropping_capture_future_cleans_staging_and_simulation_preserves_focus_without_files() {
    let root = Scratch::new();
    let mut backend = Fixture::default();
    let sequence = script(vec![capture(200)]);
    {
        let future = Engine::new(&mut backend, root.options()).run(&sequence);
        tokio::pin!(future);
        tokio::select! { _ = &mut future => panic!("finished too early"), _ = sleep(Duration::from_millis(40)) => {} }
    }
    assert!(backend.dropped.get());
    assert!(root.empty());
    let mut backend = RecordingBackend::default();
    Engine::new(&mut backend, root.options())
        .run(&script(vec![
            Action::ActivateWindow {
                window: window(),
                timeout_ms: None,
            },
            arrange(200),
            capture(200),
            text(),
        ]))
        .await
        .unwrap();
    assert!(root.empty());
    assert_eq!(backend.active, Some(window().app));
    assert_eq!(backend.active_window.as_ref().unwrap().id, 1);
    assert_eq!(backend.operations.last(), Some(&Operation::Character('x')));
}
