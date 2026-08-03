//! FFI 邊界的 DTO，與 core crate 的原生型別**刻意是兩組**（`docs/specs/ffi-contract.md §2`）。
//!
//! 轉換由本 crate 負責：`core/*` 因此完全不知道 uniffi 存在（`§5.2` 的 golden test
//! 契約不被污染），內部重構也不等於 FFI major bump。
//!
//! 全部型別額外 derive `Clone + PartialEq`：`RustEngine::mutate` 只在投影**真的改變**時
//! 才 emit（契約 C8），那個比較就發生在這些型別上。

use app_state::{AppState, BrushPreset, ToolKind};

/// 一個觸控／Pencil 取樣點。
///
/// `Vec2` 攤平成 `x`／`y`——uniffi 沒有 SIMD 概念，而 iOS 的 InputAdapter 本來就是
/// 從 `UITouch` 逐欄位讀出來的。
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct InputSample {
    pub x: f32,
    pub y: f32,
    pub t: f32,
    pub pressure: f32,
    /// 手指模式的動態來源（`architecture.md §10.2`）。
    pub radius: f32,
    pub tilt_x: f32,
    pub tilt_y: f32,
    /// 預測點只影響當前 frame，不進 oplog（契約 C4）。
    pub predicted: bool,
}

/// sRGB 8-bit。
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl From<[u8; 4]> for Rgba {
    fn from(v: [u8; 4]) -> Self {
        Self {
            r: v[0],
            g: v[1],
            b: v[2],
            a: v[3],
        }
    }
}

impl From<Rgba> for [u8; 4] {
    fn from(v: Rgba) -> Self {
        [v.r, v.g, v.b, v.a]
    }
}

/// 五支 preset 是程式碼常數不是資產驅動（`architecture.md §4.6`），
/// 所以是 enum 不是字串——Swift 端拿到的是 exhaustive switch。
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BrushId {
    SoftRound,
    Marker,
    Crayon,
    Airbrush,
    Watercolor,
}

impl From<BrushPreset> for BrushId {
    fn from(v: BrushPreset) -> Self {
        match v {
            BrushPreset::SoftRound => Self::SoftRound,
            BrushPreset::Marker => Self::Marker,
            BrushPreset::Crayon => Self::Crayon,
            BrushPreset::Airbrush => Self::Airbrush,
            BrushPreset::Watercolor => Self::Watercolor,
        }
    }
}

impl From<BrushId> for BrushPreset {
    fn from(v: BrushId) -> Self {
        match v {
            BrushId::SoftRound => Self::SoftRound,
            BrushId::Marker => Self::Marker,
            BrushId::Crayon => Self::Crayon,
            BrushId::Airbrush => Self::Airbrush,
            BrushId::Watercolor => Self::Watercolor,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum Tool {
    Brush {
        preset: BrushId,
        color: Rgba,
        size: f32,
        /// `None` 表示沿用 preset 的整筆上限。
        opacity: Option<f32>,
    },
    Eraser {
        size: f32,
    },
    Bucket {
        color: Rgba,
    },
}

impl Tool {
    pub(crate) fn apply_to(&self, app: &mut AppState) {
        match *self {
            Self::Brush {
                preset,
                color,
                size,
                opacity,
            } => {
                app.tool = ToolKind::Brush(preset.into());
                app.color = color.into();
                app.size = size;
                app.opacity = opacity;
            }
            Self::Eraser { size } => {
                app.tool = ToolKind::Eraser;
                app.size = size;
            }
            Self::Bucket { color } => {
                app.tool = ToolKind::Bucket;
                app.color = color.into();
            }
        }
    }
}

impl From<&AppState> for Tool {
    fn from(app: &AppState) -> Self {
        let color = Rgba::from(app.color);
        match app.tool {
            ToolKind::Brush(preset) => Self::Brush {
                preset: preset.into(),
                color,
                size: app.size,
                opacity: app.opacity,
            },
            ToolKind::Eraser => Self::Eraser { size: app.size },
            ToolKind::Bucket => Self::Bucket { color },
        }
    }
}

/// 遮罩模式（`E1-wgpu §7.1`）。**Debug 專用**，D4 拍板後與 `set_mask_mode`
/// 一起移除（`E1-perf §5`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MaskMode {
    /// A：只塗得進起筆處的那個區域。
    Strict,
    /// B：無條件通過。**不是** `id != REGION_LINEART`——baker 的 ID map 是滿的，
    /// 沒有保留 ID（`E1-composite §6`）。
    Loose,
}

impl From<MaskMode> for render::MaskMode {
    fn from(v: MaskMode) -> Self {
        match v {
            MaskMode::Strict => Self::Strict,
            MaskMode::Loose => Self::Loose,
        }
    }
}

/// 畫布操作只有縮放平移；加 rotation 是 major bump。
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct Transform {
    pub scale: f32,
    pub tx: f32,
    pub ty: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct Progress {
    pub colored: u32,
    pub total: u32,
}

/// 刻意只有四個欄位。`is_dirty` / `doc_revision` 不加——那是在猜 E3。
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct UiState {
    pub tool: Tool,
    /// 據實回報 pool 內容，不是假設步數（`architecture.md §4.1.1`）。
    pub can_undo: bool,
    pub can_redo: bool,
    pub progress: Progress,
}

impl From<&AppState> for UiState {
    fn from(app: &AppState) -> Self {
        Self {
            tool: Tool::from(app),
            can_undo: app.can_undo,
            can_redo: app.can_redo,
            progress: Progress {
                colored: app.colored_regions,
                total: app.total_regions,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct SurfaceHandle {
    /// `CAMetalLayer` 位址。
    pub layer_ptr: u64,
    pub width_px: u32,
    pub height_px: u32,
    /// `contentsScale`。
    pub scale: f32,
}
