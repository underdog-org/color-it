//! 檢查清冊（`specs/baker-core-design.md §4.1`）。
//!
//! **階段內不 fail-fast**：跑完該階段全部檢查才決定，繪師一次拿到所有問題。
//! **階段間 fail-fast**：母帶有 Error 就不進降採樣，後面的結果沒有意義。

pub mod master;
pub mod output;
