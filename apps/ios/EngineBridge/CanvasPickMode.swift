//
//  CanvasPickMode.swift
//  EngineBridge
//

import Observation

/// 吸管的待命旗標與取回的顏色。**Bridge 專屬**，不在 FFI 表面上（`contracts.md` C7）。
///
/// 為什麼不是 `UiState` 的一個欄位：待命與否不改變任何引擎狀態，按下吸管到真的取到色
/// 之間什麼都沒發生。放進 `UiState` 會讓每次切吸管都 emit 一次狀態變更。
///
/// 為什麼不是 Shell 自己的 `@State`：真正判斷「這一下是取色還是塗抹」的是
/// `EngineCanvasView.touchesBegan`，而輸入不經過 Shell（`E1-input §3`）。
@Observable
public final class CanvasPickMode {
    /// `true` 時下一次點畫布走 `pickColor`，取完自動歸 `false`。
    public var isArmed = false
    /// 最近一次取到的顏色。Shell 觀察它。
    public var picked: Rgba?

    public init() {}
}
