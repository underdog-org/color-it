//! FFI facade：生命週期、鎖與 DTO 轉換。**沒有業務邏輯**。
//!
//! 業務邏輯住 `document`（狀態）與 `render`（GPU）。本檔只做三件事：
//! 把 FFI 的 DTO 翻成 core 的型別、在同一把鎖底下協調兩者、把 `Effect` 翻成 GPU 動作。
//! 行為表見 `docs/specs/ffi-contract.md §5`，E1 的接線見 `docs/specs/E1-bucket.md §3`。

use std::fs::File;
use std::io::BufReader;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use colorpack::ColorPack;
use document::{Effect, Op};
use render::{Frame, SurfaceHandle as RenderSurfaceHandle, encode};

use crate::error::EngineError;
use crate::ffi::{InputSample, Rgba, SurfaceHandle, Tool, Transform, UiState};
use crate::inner::{CANVAS_BACKGROUND, Inner};
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

    /// 測試專用：不經過 surface 就把 GPU 與文件資源建起來。
    ///
    /// 正式路徑只有 `attach_surface` 一條（`E1-wgpu §2`），而它需要真的
    /// `CAMetalLayer`。`tap` 這條線要驗，就得有另一個入口——但**只在測試存在**，
    /// 不進 FFI 表面。
    #[cfg(test)]
    fn prepare_gpu(&self) -> Result<(), render::RenderError> {
        let mut inner = self.lock();
        let Inner { render, pack, .. } = &mut *inner;
        render.prepare_document(pack)
    }
}

#[uniffi::export]
impl RustEngine {
    /// 真的解析 `.colorpack`——`total_regions` 與 `region_ids` 都從這裡來。
    ///
    /// **不吃 surface**：`new` 因此能在無 GPU 的環境跑，那是 headless 測試與
    /// 「App 啟動不必等 GPU」的前提（契約 C5）。GPU 初始化的唯一時機是
    /// `attach_surface`（`E1-wgpu §2`）。
    #[uniffi::constructor]
    pub fn new(pack_path: String, doc_path: Option<String>) -> Result<Arc<Self>, EngineError> {
        // 存檔在 E3；E1 每次都從 pack 的初始狀態開始。
        let _ = doc_path;

        let file = File::open(&pack_path).map_err(|e| EngineError::Pack {
            detail: format!("開不了資產包 {pack_path}：{e}"),
        })?;
        let pack = ColorPack::open(BufReader::new(file)).map_err(|e| EngineError::Pack {
            detail: format!("{pack_path}：{e}"),
        })?;

        Ok(Arc::new(Self {
            inner: Mutex::new(Inner::new(pack)),
        }))
    }

    /// **E1 起真的會失敗**（`E1-wgpu §2.2`）：adapter 取不到、device 建不出來、
    /// surface 格式不支援。失敗時 `surface` 維持 `None`，畫作仍在 engine 裡——
    /// Swift 端顯示錯誤態即可，不需要 crash（`E1-input §8`）。
    pub fn attach_surface(&self, handle: SurfaceHandle) -> Result<(), EngineError> {
        let mut result = Ok(());
        self.mutate(|inner| {
            let target = RenderSurfaceHandle {
                layer_ptr: handle.layer_ptr,
                width_px: handle.width_px,
                height_px: handle.height_px,
                scale: handle.scale,
            };
            match unsafe { inner.render.attach_surface(target, &inner.pack) } {
                Ok(()) => {
                    inner.surface = Some(handle);
                    inner.refit();
                }
                Err(e) => {
                    result = Err(EngineError::Surface {
                        detail: e.to_string(),
                    });
                }
            }
        });
        result
    }

    pub fn resize_surface(&self, width_px: u32, height_px: u32, scale: f32) {
        self.mutate(|inner| {
            if let Some(surface) = inner.surface.as_mut() {
                surface.width_px = width_px;
                surface.height_px = height_px;
                surface.scale = scale;
            }
            inner.render.resize_surface(width_px, height_px);
            inner.refit();
        });
    }

    /// **只丟 surface。** device 與文件資源留著——切出 App 再回來，畫作還在（契約 C5）。
    pub fn detach_surface(&self) {
        self.mutate(|inner| {
            inner.surface = None;
            inner.render.detach_surface();
        });
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

    /// 油漆桶。`x` / `y` 是**螢幕像素**（`E1-bucket §4.1`），乘 `contentsScale`
    /// 是 Bridge 的責任。
    ///
    /// 三步：逆變換 → region ID → `document.apply`。`document` 回的 `Effect` 才被
    /// 翻成 GPU 動作——**`document` 不認得 `render`**（`E1-bucket §3`），中間永遠隔著這裡。
    ///
    /// 沒有 GPU 資源時（尚未 `attach_surface`）落空：`region_ids` 的唯一副本住在
    /// `DocumentResources`（`E1-wgpu §5.1`），engine 不另開一份。畫面都還沒有的
    /// 時候填色也無從看見。
    pub fn tap(&self, x: f32, y: f32) {
        self.mutate(|inner| {
            let canvas = inner.transform.canvas_pos([x, y]);
            // 畫布外 → `None`，不 clamp（`E1-bucket §4.3`）。
            let Some(region_id) = inner.render.resources().and_then(|r| r.region_at(canvas)) else {
                return;
            };

            let op = Op::Fill {
                region_id,
                color: inner.app.color,
            };
            // 同色重填與不存在的 ID 都回 `Effect::None`——什麼都不做，也不 emit。
            let Effect::Filled {
                region_id,
                color,
                prev,
                bbox,
            } = inner.doc.apply(op)
            else {
                return;
            };

            // 進度是 `document` 的投影，不是第二份計數器。
            inner.app.colored_regions = inner.doc.colored_regions();
            inner.render.fill(region_id, color, prev, bbox, canvas);
        });
    }

    pub fn undo(&self) {
        log_once!("[colorlull] undo 尚未實作（排程 E3），本次為 no-op");
    }

    pub fn redo(&self) {
        log_once!("[colorlull] redo 尚未實作（排程 E3），本次為 no-op");
    }

    /// 由 FrameDriver 每 frame 呼叫（`E1-input §2.1`），所以 infallible——
    /// Swift 端不會想每 frame `try`。掉 frame 與取不到 drawable 都不是錯誤，
    /// 由 `render` 內部吸收。
    ///
    /// E1 只有 Pass 3 Composite；Pass 1／2 由 `E1-stroke` 插在它前面。
    pub fn render(&self) {
        self.mutate(|inner| {
            let frame = Frame {
                transform: inner.transform,
                screen_size: inner.screen_size(),
                background: CANVAS_BACKGROUND,
                // 進行中筆畫的顏色。Pass 1 還沒接上時 `T_wet` 恆為空，
                // 這個值不影響畫面，但先送對的東西省得 E1-stroke 再找一次。
                brush_color: encode(inner.app.color),
            };
            if let Err(e) = inner.render.render(frame) {
                log_once!("[colorlull] render 失敗（僅報一次）：{e}");
            }
        });
    }

    /// E1 的 transform 由 `attach` / `resize` 自己算 fit-to-screen，這支是 E2
    /// 縮放平移的入口。**畫布逆變換的真相在 Rust**，Swift 端不另存一份
    /// （`E1-input §5`）。
    pub fn set_viewport(&self, transform: Transform) {
        self.mutate(|inner| inner.transform = transform.into());
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

    use colorpack::manifest::{Aspect, Category, Difficulty, Manifest};
    use colorpack::region::RegionEntry;

    use super::*;
    use crate::ffi::{BrushId, Progress};

    /// fixture 的畫布：4×4，左兩欄是 region 0、右兩欄是 region 1。
    const CANVAS: u32 = 4;
    const TOTAL_REGIONS: u32 = 2;

    fn temp_path(tag: &str) -> std::path::PathBuf {
        static SEQ: AtomicU32 = AtomicU32::new(0);
        std::env::temp_dir().join(format!(
            "colorlull-{tag}-{}-{}.colorpack",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ))
    }

    /// 最小但**合法**的 `.colorpack`：`ColorPack::open` 會驗 `content_hash`，
    /// 而 `DocumentResources` 會真的去解線稿 PNG——兩者都糊弄不過去。
    fn write_pack(path: &std::path::Path) {
        let region_ids: Vec<u16> = (0..CANVAS * CANVAS)
            .map(|i| u16::from(i % CANVAS >= CANVAS / 2))
            .collect();
        let regions = (0..TOTAL_REGIONS)
            .map(|id| RegionEntry {
                id,
                centroid: [0, 0],
                area: (CANVAS * CANVAS / 2),
                bbox: [id * CANVAS / 2, 0, CANVAS / 2, CANVAS],
                suggested_color: "#FFFFFF".to_owned(),
            })
            .collect();

        let mut pack = ColorPack {
            manifest: Manifest {
                schema_version: "1.0".to_owned(),
                id: "engine-test".to_owned(),
                content_hash: String::new(),
                canvas_size: [CANVAS, CANVAS],
                aspect: Aspect::Square,
                region_count: TOTAL_REGIONS,
                difficulty: Difficulty::Easy,
                category: Category::Animal,
                has_shade: false,
                palette: vec![],
            },
            regions,
            region_ids,
            lineart_png: transparent_png(CANVAS, CANVAS),
            shade_png: None,
            thumb_jpg: vec![],
        };
        pack.write_to(std::fs::File::create(path).unwrap()).unwrap();
    }

    fn transparent_png(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&vec![0u8; (w * h * 4) as usize])
            .unwrap();
        writer.finish().unwrap();
        out
    }

    fn engine() -> Arc<RustEngine> {
        let path = temp_path("engine");
        write_pack(&path);
        let engine = RustEngine::new(path.to_string_lossy().into_owned(), None).unwrap();
        std::fs::remove_file(&path).unwrap();
        engine
    }

    /// `tap` 需要 `DocumentResources` 才有 `region_ids`，而正式路徑只有
    /// `attach_surface` 一條——它要真的 `CAMetalLayer`。這裡走測試專用的入口。
    fn engine_with_gpu() -> Arc<RustEngine> {
        let engine = engine();
        engine.prepare_gpu().expect("需要可用的 GPU");
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

    /// iOS 測試 fixture 的相對路徑。**進 git**——它是 `EngineBridgeTests` 唯一能
    /// 拿到合法 `.colorpack` 的方式（zip 容器 ＋ `content_hash` 的實作只存在於
    /// Rust，Swift 端重寫一份就違反「一份契約只能存在一次」）。
    const IOS_FIXTURE: &str = "../../apps/ios/EngineBridgeTests/Fixtures/test.colorpack";

    /// 重新產生 iOS 的 fixture。schema 版本或容器格式改了才需要跑：
    ///
    /// ```text
    /// cargo test -p colorlull-engine regenerate_ios_fixture -- --ignored
    /// ```
    #[test]
    #[ignore = "產生檔案，不是驗證"]
    fn regenerate_ios_fixture() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(IOS_FIXTURE);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        write_pack(&path);
    }

    /// checked-in 的 fixture 會隨著 schema 演進而失效，而失效的症狀出現在
    /// Xcode 裡（7 條 Swift 測試同時紅），離原因很遠。這條把它拉回 Rust 這側。
    #[test]
    fn ios_fixture_still_matches_the_current_schema() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(IOS_FIXTURE);
        let engine = RustEngine::new(path.to_string_lossy().into_owned(), None)
            .expect("fixture 過期了，跑 `regenerate_ios_fixture -- --ignored` 重新產生");

        assert_eq!(engine.state().progress.total, TOTAL_REGIONS);
    }

    /// `total` 現在是資產包裡的真實 region 數，不再是 mock 常數。
    #[test]
    fn total_regions_comes_from_the_pack() {
        assert_eq!(
            engine().state().progress,
            Progress {
                colored: 0,
                total: TOTAL_REGIONS
            }
        );
    }

    #[test]
    fn new_rejects_a_file_that_is_not_a_pack() {
        let path = temp_path("garbage");
        std::fs::write(&path, b"stub").unwrap();
        let result = RustEngine::new(path.to_string_lossy().into_owned(), None);
        std::fs::remove_file(&path).unwrap();

        assert!(matches!(result, Err(EngineError::Pack { .. })));
    }

    #[test]
    fn new_rejects_a_missing_pack() {
        assert!(matches!(
            RustEngine::new("/nonexistent.colorpack".to_owned(), None),
            Err(EngineError::Pack { .. })
        ));
    }

    /// 一次 tap ＝ 一次 `document.apply`，進度是 `document` 的投影。
    #[test]
    fn tap_fills_the_region_under_the_point() {
        let engine = engine_with_gpu();
        let recorder = Arc::new(Recorder::default());
        engine.set_state_listener(Some(recorder.clone()));

        // 左半：region 0。
        engine.tap(0.5, 0.5);
        assert_eq!(engine.state().progress.colored, 1);

        // 右半：region 1。
        engine.tap(3.5, 0.5);
        assert_eq!(engine.state().progress.colored, TOTAL_REGIONS);
        assert_eq!(recorder.seen().len(), 2);
    }

    /// 同色重填回 `Effect::None`——狀態沒變，所以不 emit（契約 C8）。
    #[test]
    fn tapping_the_same_region_twice_changes_nothing() {
        let engine = engine_with_gpu();
        let recorder = Arc::new(Recorder::default());
        engine.set_state_listener(Some(recorder.clone()));

        engine.tap(0.5, 0.5);
        engine.tap(1.5, 3.5); // 同一區的另一點
        assert_eq!(engine.state().progress.colored, 1);
        assert_eq!(recorder.seen().len(), 1);
    }

    /// 畫布外**不 clamp**（`E1-bucket §4.3`）——clamp 會讓誤觸填到邊緣區域。
    #[test]
    fn tap_outside_the_canvas_does_nothing() {
        let engine = engine_with_gpu();

        engine.tap(-1.0, 0.5);
        engine.tap(0.5, CANVAS as f32 + 1.0);
        assert_eq!(engine.state().progress.colored, 0);
    }

    /// 還沒 `attach_surface` 就 tap：`region_ids` 還不存在，落空而不是 panic。
    #[test]
    fn tap_before_attach_is_a_noop() {
        let engine = engine();
        engine.tap(0.5, 0.5);
        assert_eq!(engine.state().progress.colored, 0);
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
        // 沒有 surface 時 `render` 直接回 `Ok`——資源都還在（契約 C5）。
        engine.render();
        engine.detach_surface();
    }

    /// `set_viewport` 覆寫 fit-to-screen 的結果，而 `tap` 的逆變換吃的就是它。
    #[test]
    fn set_viewport_moves_the_tap_target() {
        let engine = engine_with_gpu();

        // 放大兩倍並右移：螢幕 (5, 1) 落在畫布 (2.5, 0.5)，即右半的 region 1。
        engine.set_viewport(Transform {
            scale: 2.0,
            tx: 0.0,
            ty: 0.0,
        });
        engine.tap(5.0, 1.0);
        assert_eq!(engine.state().progress.colored, 1);

        // 同一個螢幕座標，在恆等變換下落在畫布外——什麼都不該發生。
        engine.set_viewport(Transform {
            scale: 1.0,
            tx: 0.0,
            ty: 0.0,
        });
        engine.tap(5.0, 1.0);
        assert_eq!(engine.state().progress.colored, 1);
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
