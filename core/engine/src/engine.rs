//! FFI facade：生命週期、鎖與 DTO 轉換。**沒有業務邏輯**。
//!
//! S0 是 headless mock，行為表見 `docs/specs/ffi-contract.md §5`。

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use app_state::AppState;

use crate::error::EngineError;
use crate::ffi::{InputSample, Rgba, SurfaceHandle, Tool, Transform, UiState};
use crate::inner::{Inner, MOCK_TOTAL_REGIONS};
use crate::listener::StateListener;

/// 未實作的 infallible 方法用它報一次到——不是 panic、不是回傳錯誤。
///
/// `Once` 的 static 展開在各呼叫點自己的作用域，所以 `render()` 不需要為了記 log
/// 而每 frame 取鎖，第二次之後連字串都不會碰。
macro_rules! log_once {
    ($($arg:tt)*) => {{
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| eprintln!($($arg)*));
    }};
}

#[derive(uniffi::Object)]
pub struct RustEngine {
    inner: Mutex<Inner>,
}

impl RustEngine {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// **唯一**的狀態變更入口。
    ///
    /// 「發送前先釋放鎖」（契約 C2）如果靠紀律維持一定會被打破，所以做成結構：
    /// 沒有第二條路可以改 `Inner`，也就沒有「這個方法該不該走 mutate」的判斷。
    fn mutate(&self, f: impl FnOnce(&mut Inner)) {
        let pending = {
            let mut inner = self.lock();
            f(&mut inner);

            let projected = UiState::from(&inner.app);
            if inner.last_emitted.as_ref() == Some(&projected) {
                // 契約 C8：`append_samples` 一 frame 一次，120Hz 下等於每秒 120 次
                // 內容完全相同的回呼——而 stroke 狀態根本不在 `UiState` 裡。
                None
            } else {
                inner.last_emitted = Some(projected.clone());
                inner.listener.clone().map(|l| (l, projected))
            }
        }; // ← 鎖在此釋放

        if let Some((listener, state)) = pending {
            listener.on_state(state);
        }
    }
}

#[uniffi::export]
impl RustEngine {
    /// S0 只檢查 `pack_path` 存在，不解析 `.colorpack`（格式在 M1）。
    ///
    /// 不吃 surface：`new` 因此能在無 GPU 的環境跑，那是 headless mock 與 CI
    /// 單元測試的前提（契約 C5）。
    #[uniffi::constructor]
    pub fn new(pack_path: String, doc_path: Option<String>) -> Result<Arc<Self>, EngineError> {
        let _ = doc_path;

        if !Path::new(&pack_path).exists() {
            return Err(EngineError::Pack {
                detail: format!("找不到資產包：{pack_path}"),
            });
        }

        let app = AppState {
            total_regions: MOCK_TOTAL_REGIONS,
            ..AppState::default()
        };
        Ok(Arc::new(Self {
            inner: Mutex::new(Inner::new(app)),
        }))
    }

    pub fn attach_surface(&self, handle: SurfaceHandle) -> Result<(), EngineError> {
        self.mutate(|inner| inner.surface = Some(handle));
        Ok(())
    }

    pub fn resize_surface(&self, width_px: u32, height_px: u32, scale: f32) {
        self.mutate(|inner| {
            if let Some(surface) = inner.surface.as_mut() {
                surface.width_px = width_px;
                surface.height_px = height_px;
                surface.scale = scale;
            }
        });
    }

    pub fn detach_surface(&self) {
        self.mutate(|inner| inner.surface = None);
    }

    pub fn set_tool(&self, tool: Tool) {
        self.mutate(|inner| tool.apply_to(&mut inner.app));
    }

    pub fn pick_color(&self, x: f32, y: f32) -> Rgba {
        let _ = (x, y);
        Rgba::from(self.lock().app.color)
    }

    pub fn begin_stroke(&self, s: InputSample) {
        let _ = s;
        self.mutate(|inner| inner.stroke_active = true);
    }

    /// 唯一的高頻路徑，批次進來（契約 C3）。S0 維護狀態機、樣本丟棄。
    pub fn append_samples(&self, s: Vec<InputSample>) {
        let _ = s;
        // 沒 begin 過就 append 只是 Bridge 的事件順序問題，不是 panic 的理由；
        // S0 兩種情況都丟棄樣本。仍走 mutate，維持「只有一條路能碰 Inner」。
        self.mutate(|_| {});
    }

    pub fn end_stroke(&self) {
        self.mutate(|inner| inner.stroke_active = false);
    }

    pub fn cancel_stroke(&self) {
        self.mutate(|inner| inner.stroke_active = false);
    }

    pub fn tap(&self, x: f32, y: f32) {
        let _ = (x, y);
        self.mutate(|inner| {
            inner.app.mark_region_colored();
        });
    }

    pub fn undo(&self) {
        log_once!("[colorlull] undo 尚未實作（排程 E3），本次為 no-op");
    }

    pub fn redo(&self) {
        log_once!("[colorlull] redo 尚未實作（排程 E3），本次為 no-op");
    }

    pub fn render(&self) {
        log_once!("[colorlull] render 尚未實作（排程 E1），本次為 no-op");
    }

    pub fn set_viewport(&self, transform: Transform) {
        let _ = transform;
        log_once!("[colorlull] set_viewport 尚未實作（排程 E1），本次為 no-op");
    }

    pub fn state(&self) -> UiState {
        UiState::from(&self.lock().app)
    }

    /// 單一 listener、後設覆蓋前設；`None` 是明確的 detach 路徑——
    /// 否則 Swift 端的 retain cycle 沒有解。
    pub fn set_state_listener(&self, listener: Option<Arc<dyn StateListener>>) {
        self.mutate(|inner| inner.listener = listener);
    }

    pub fn save(&self) -> Result<(), EngineError> {
        Err(EngineError::not_implemented("save", "E3"))
    }

    pub fn export_png(&self) -> Result<Vec<u8>, EngineError> {
        Err(EngineError::not_implemented("export_png", "E1"))
    }

    pub fn export_timelapse(&self) -> Result<Vec<u8>, EngineError> {
        Err(EngineError::not_implemented("export_timelapse", "E3"))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Mutex, Weak, mpsc};
    use std::time::Duration;

    use super::*;
    use crate::ffi::{BrushId, Progress};

    fn engine() -> Arc<RustEngine> {
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "colorlull-s0-{}-{}.colorpack",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, b"stub").unwrap();
        let engine = RustEngine::new(path.to_string_lossy().into_owned(), None).unwrap();
        std::fs::remove_file(&path).unwrap();
        engine
    }

    fn marker(size: f32) -> Tool {
        Tool::Brush {
            preset: BrushId::Marker,
            color: Rgba {
                r: 0xff,
                g: 0x00,
                b: 0x66,
                a: 0xff,
            },
            size,
            opacity: Some(0.6),
        }
    }

    #[derive(Default)]
    struct Recorder(Mutex<Vec<UiState>>);

    impl Recorder {
        fn seen(&self) -> Vec<UiState> {
            self.0.lock().unwrap().clone()
        }
    }

    impl StateListener for Recorder {
        fn on_state(&self, state: UiState) {
            self.0.lock().unwrap().push(state);
        }
    }

    #[test]
    fn set_tool_round_trips_through_state() {
        let engine = engine();
        engine.set_tool(marker(9.0));
        assert_eq!(engine.state().tool, marker(9.0));

        engine.set_tool(Tool::Eraser { size: 40.0 });
        assert_eq!(engine.state().tool, Tool::Eraser { size: 40.0 });
    }

    #[test]
    fn set_tool_emits_once_with_current_state() {
        let engine = engine();
        let recorder = Arc::new(Recorder::default());
        engine.set_state_listener(Some(recorder.clone()));

        engine.set_tool(marker(9.0));

        let seen = recorder.seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0], engine.state());
    }

    #[test]
    fn listener_may_reenter_engine_without_deadlock() {
        struct Reentrant {
            engine: Weak<RustEngine>,
            seen: Mutex<Vec<UiState>>,
        }

        impl StateListener for Reentrant {
            fn on_state(&self, state: UiState) {
                let engine = self.engine.upgrade().unwrap();
                assert_eq!(engine.state(), state);
                self.seen.lock().unwrap().push(state);
            }
        }

        let engine = engine();
        let listener = Arc::new(Reentrant {
            engine: Arc::downgrade(&engine),
            seen: Mutex::new(Vec::new()),
        });
        engine.set_state_listener(Some(listener.clone()));

        let (tx, rx) = mpsc::channel();
        let worker = engine.clone();
        std::thread::spawn(move || {
            worker.set_tool(marker(9.0));
            let _ = tx.send(());
        });

        rx.recv_timeout(Duration::from_secs(5))
            .expect("回呼中呼叫 state() 死鎖了");
        assert_eq!(listener.seen.lock().unwrap().len(), 1);
    }

    #[test]
    fn detaching_listener_stops_callbacks() {
        let engine = engine();
        let recorder = Arc::new(Recorder::default());
        engine.set_state_listener(Some(recorder.clone()));

        engine.set_tool(marker(9.0));
        engine.set_state_listener(None);
        engine.set_tool(marker(30.0));

        assert_eq!(recorder.seen().len(), 1);
    }

    #[test]
    fn tap_advances_progress_and_saturates() {
        let engine = engine();
        let recorder = Arc::new(Recorder::default());
        engine.set_state_listener(Some(recorder.clone()));

        engine.tap(1.0, 2.0);
        assert_eq!(
            engine.state().progress,
            Progress {
                colored: 1,
                total: MOCK_TOTAL_REGIONS
            }
        );
        assert_eq!(recorder.seen().len(), 1);

        for _ in 1..MOCK_TOTAL_REGIONS {
            engine.tap(1.0, 2.0);
        }
        assert_eq!(engine.state().progress.colored, MOCK_TOTAL_REGIONS);
        assert_eq!(recorder.seen().len(), MOCK_TOTAL_REGIONS as usize);

        engine.tap(1.0, 2.0);
        assert_eq!(engine.state().progress.colored, MOCK_TOTAL_REGIONS);
        assert_eq!(recorder.seen().len(), MOCK_TOTAL_REGIONS as usize);
    }

    #[test]
    fn stroke_state_machine_tolerates_out_of_order_events() {
        let engine = engine();
        let sample = InputSample {
            x: 1.0,
            y: 2.0,
            t: 0.0,
            pressure: 0.5,
            radius: 4.0,
            tilt_x: 0.0,
            tilt_y: 0.0,
            predicted: false,
        };

        engine.append_samples(vec![sample.clone()]);
        engine.end_stroke();

        engine.begin_stroke(sample.clone());
        engine.cancel_stroke();
        engine.end_stroke();
        engine.append_samples(vec![sample]);

        assert!(!engine.lock().stroke_active);
    }

    #[test]
    fn unimplemented_methods_error_or_noop() {
        let engine = engine();

        assert!(matches!(
            engine.save(),
            Err(EngineError::NotImplemented { .. })
        ));
        assert!(matches!(
            engine.export_png(),
            Err(EngineError::NotImplemented { .. })
        ));
        assert!(matches!(
            engine.export_timelapse(),
            Err(EngineError::NotImplemented { .. })
        ));

        engine.undo();
        engine.redo();
        engine.render();
        engine.set_viewport(Transform {
            scale: 1.0,
            tx: 0.0,
            ty: 0.0,
        });
        engine.detach_surface();
    }

    /// 契約 C8。
    #[test]
    fn identical_state_does_not_emit_twice() {
        let engine = engine();
        let recorder = Arc::new(Recorder::default());
        engine.set_state_listener(Some(recorder.clone()));

        engine.set_tool(marker(9.0));
        engine.set_tool(marker(9.0));
        assert_eq!(recorder.seen().len(), 1);

        engine.begin_stroke(InputSample {
            x: 0.0,
            y: 0.0,
            t: 0.0,
            pressure: 1.0,
            radius: 4.0,
            tilt_x: 0.0,
            tilt_y: 0.0,
            predicted: false,
        });
        for _ in 0..8 {
            engine.append_samples(vec![]);
        }
        engine.end_stroke();
        assert_eq!(recorder.seen().len(), 1, "stroke 狀態不在 UiState 裡");
    }
}
