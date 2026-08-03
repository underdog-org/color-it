//! 目前工具、顏色、筆刷大小、進度、UiState 投影。不負責持久化。
//!
//! 這裡是「目前工具、顏色、筆刷大小、進度」的**唯一**住處（`architecture.md §5.1`）。
//! `engine` 只做 `From<&AppState> for ffi::UiState` 的投影，不持有第二份副本——
//!
//! 欄位由已定案的 `UiState` 反推，見 `docs/specs/ffi-contract.md §4`。

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrushPreset {
    #[default]
    SoftRound,
    Marker,
    Crayon,
    Airbrush,
    Watercolor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Brush(BrushPreset),
    Eraser,
    Bucket,
}

impl Default for ToolKind {
    fn default() -> Self {
        Self::Brush(BrushPreset::default())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    pub tool: ToolKind,
    /// sRGB 8-bit RGBA。
    pub color: [u8; 4],
    pub size: f32,
    /// 使用者對整筆上限的覆寫；`None` 表示沿用 preset（`architecture.md §6`）。
    pub opacity: Option<f32>,
    pub colored_regions: u32,
    pub total_regions: u32,
    pub can_undo: bool,
    pub can_redo: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            tool: ToolKind::default(),
            color: [0x1a, 0x1a, 0x1a, 0xff],
            size: 24.0,
            opacity: None,
            colored_regions: 0,
            total_regions: 0,
            can_undo: false,
            can_redo: false,
        }
    }
}

impl AppState {
    pub fn mark_region_colored(&mut self) -> bool {
        if self.colored_regions >= self.total_regions {
            return false;
        }
        self.colored_regions += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_saturates_at_total() {
        let mut state = AppState {
            total_regions: 2,
            ..AppState::default()
        };

        assert!(state.mark_region_colored());
        assert!(state.mark_region_colored());
        assert!(!state.mark_region_colored(), "到頂後不得再增加");
        assert_eq!(state.colored_regions, 2);
    }

    #[test]
    fn progress_saturates_when_total_is_zero() {
        let mut state = AppState::default();
        assert!(!state.mark_region_colored());
        assert_eq!(state.colored_regions, 0);
    }
}
