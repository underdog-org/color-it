//! FFI 邊界的錯誤型別。
//!
//! 哪些方法 fallible 是契約的一部分，不能因為 S0 是 mock 就臨時挪動：
//! `new` / `attach_surface` / `save` / `export_*` fallible，其餘一律 infallible。
//! `render()` 每 frame 呼叫，Swift 端不會想每 frame `try`。

/// 跨 FFI 的錯誤。變體刻意少——Swift 端要能 exhaustive switch。
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum EngineError {
    #[error("尚未實作：{feature}（排程 {milestone}）")]
    NotImplemented { feature: String, milestone: String },

    #[error("資產包載入失敗：{detail}")]
    Pack { detail: String },

    /// `attach_surface` 專屬。S0 永遠回 `Ok`，**E1 起真的會失敗**
    /// （`E1-wgpu §2.2`）——adapter 取不到、device 建不出來、surface 格式不支援。
    /// 與 `Pack` 分開是因為使用者能做的事不同：資產包壞了要重下載，
    /// surface 失敗則是裝置／時機問題，畫作還在 engine 裡。
    #[error("繪圖表面建立失敗：{detail}")]
    Surface { detail: String },

    #[error("I/O 失敗：{detail}")]
    Io { detail: String },
}

impl EngineError {
    pub(crate) fn not_implemented(feature: &str, milestone: &str) -> Self {
        Self::NotImplemented {
            feature: feature.to_owned(),
            milestone: milestone.to_owned(),
        }
    }
}
