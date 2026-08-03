//! `RustEngine` 鎖起來的那一坨狀態。
//!
//! 業務狀態一律住 `app_state::AppState`（`architecture.md §5.1`），文件狀態住
//! `document::Document`（鐵律 #3 的單一寫入口），GPU 狀態住 `render::RenderContext`。
//! 這裡只負責把三者放在同一把鎖底下，**自己不留任何第二份真相**。

use std::sync::Arc;

use app_state::AppState;
use colorpack::ColorPack;
use document::Document;
use render::{MaskMode, MaskUniform, RenderContext, Transform as RenderTransform, encode};

use crate::brush::ActiveStroke;
use crate::ffi::{SurfaceHandle, Transform, UiState};
use crate::listener::StateListener;

/// 畫布外的背景色（`E1-composite §4`）。**刻意不是 `PAPER_WHITE`**——
/// 兩者相同的話使用者看不出畫布邊界在哪。編碼值 RGBA。
pub(crate) const CANVAS_BACKGROUND: [f32; 4] = [0.14, 0.14, 0.15, 1.0];

pub(crate) struct Inner {
    pub app: AppState,
    pub listener: Option<Arc<dyn StateListener>>,
    /// 文件狀態的唯一住處。`palette` 的真相在這裡，GPU 上的 `Buf_palette` 是它的投影。
    pub doc: Document,
    /// `attach_surface` 每次都要把 pack 交給 `render`（資源在第一次 attach 時配置，
    /// 之後 detach 不丟），所以整份留著。
    pub pack: ColorPack,
    pub render: RenderContext,
    /// 記下來只為了 `Frame.screen_size`——GPU 側的 surface 生命週期在 `render` 裡。
    pub surface: Option<SurfaceHandle>,
    /// `set_viewport` 送進來的那一份。**Swift 端不另存**（`E1-input §5`）。
    pub transform: RenderTransform,
    /// stroke 狀態機只活在這裡，**不在 `UiState`**——所以 `append_samples` 不會 emit。
    /// `Some` 即「有一筆正在進行」，不另留一個 bool（兩份真相會不同步）。
    pub stroke: Option<ActiveStroke>,
    /// jitter 的 seed 來源。**遞增而非亂數**：E3 的縮時重播要能重現原作，
    /// 而重播的前提是同一筆拿到同一個 seed。
    pub stroke_seq: u32,
    /// D4 的比較開關（`E1-perf §5`）。真相在這裡，GPU 上的 mask uniform 是它的投影。
    pub mask_mode: MaskMode,
    pub last_emitted: Option<UiState>,
}

impl Inner {
    pub(crate) fn new(pack: ColorPack) -> Self {
        let doc = Document::from_pack(&pack);
        let app = AppState {
            total_regions: doc.total_regions(),
            ..AppState::default()
        };
        let last_emitted = Some(UiState::from(&app));
        Self {
            app,
            listener: None,
            doc,
            pack,
            render: RenderContext::new(),
            surface: None,
            // 第一次 `set_viewport` 之前的佔位值。恆等變換讓 `canvas_pos`
            // 退化成「螢幕像素 == 畫布像素」，而不是產生 NaN。
            transform: RenderTransform {
                scale: 1.0,
                tx: 0.0,
                ty: 0.0,
            },
            stroke: None,
            stroke_seq: 0,
            // 起始是嚴格模式：E1 的產品預設是「受區域約束的塗抹」，
            // 寬鬆模式是 D4 拿來對照的那一邊（`prd.md §3`）。
            mask_mode: MaskMode::Strict,
            last_emitted,
        }
    }

    /// `Frame.screen_size`。沒有 surface 時給 0——`render` 在那個狀態下本來就會
    /// 提早回傳，這個值不會被用到。
    pub(crate) fn screen_size(&self) -> [u32; 2] {
        self.surface.map_or([0, 0], |s| [s.width_px, s.height_px])
    }

    /// 把 mask 的真相推到 GPU。一次 `write_buffer`，不重建 pipeline（`E1-wgpu §7.1`）。
    ///
    /// `active_region_id` 是**起筆處**的 region（`E1-bucket §4.4`），沒有進行中的
    /// 筆畫時給 0——那個值在 `T_wet` 為空時不影響任何像素。
    pub(crate) fn sync_mask(&self) {
        self.render.set_mask(MaskUniform {
            mode: self.mask_mode as u32,
            active_region_id: self.stroke.as_ref().map_or(0, |s| s.region_id),
        });
    }

    /// composite 第 ④ 層的顏色（`E1-composite §3`）。
    ///
    /// 進行中的筆畫用**整筆 opacity 當 alpha**，不是顏色自己的 alpha：Pass 2 commit
    /// 乘的是 `opacity`，兩邊不一致的話筆畫會在抬筆瞬間變深或變淡。
    pub(crate) fn brush_color(&self) -> [f32; 4] {
        match self.stroke.as_ref() {
            Some(s) => [s.color[0], s.color[1], s.color[2], s.opacity],
            None => encode(self.app.color),
        }
    }

    /// 重算 fit-to-screen 的 transform。**E1 沒有縮放平移**（`E1-composite §4`），
    /// 所以 attach／resize 之後直接重算是對的；`set_viewport` 覆寫它的路留給 E2。
    pub(crate) fn refit(&mut self) {
        let screen = self.screen_size();
        if screen[0] == 0 || screen[1] == 0 {
            return;
        }
        self.transform = RenderTransform::fit(self.pack.manifest.canvas_size, screen);
    }
}

impl From<Transform> for RenderTransform {
    fn from(t: Transform) -> Self {
        Self {
            scale: t.scale,
            tx: t.tx,
            ty: t.ty,
        }
    }
}
