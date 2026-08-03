//
//  InputAdapter.swift
//  EngineBridge
//
//  `docs/specs/E1-input.md §4`–`§7` 的實作。
//
//  **演算法一律不在這裡。**`majorRadius` → `pressure` 的正規化在 `core/stroke §5`，
//  畫布逆變換在 Rust 的 `Transform::canvas_pos`。本檔只決定「送什麼進去」。
//

import UIKit

/// `UITouch` → `InputSample` 的翻譯，外加一 frame 一批的緩衝。
///
/// 它不呼叫 engine——時機表在 `EngineCanvasView`（§3），這裡只累積與清空。
final class InputAdapter {
    /// 真實樣本。累積到 FrameDriver 來收為止。
    private var real: [InputSample] = []

    /// **預測點每 frame 重新產生，不累積**（§4）。UIKit 每次 touch 事件都給一組
    /// 全新的預測，上一批沒有任何保存價值——所以是覆寫，不是 append。
    private var predicted: [InputSample] = []

    /// `UITouch.timestamp` 是 `systemUptime`，開機幾天後是數十萬秒，而
    /// `InputSample.t` 是 `f32`（有效位數約 7 位）——`432000.0` 的下一個可表示值
    /// 差 0.03 秒，One-Euro 的 `dt` 會直接爛掉（§4.1）。所以相對筆畫起點歸零。
    private var strokeStart: TimeInterval?

    /// **只追蹤第一根手指**（§7）。E1 沒有縮放平移手勢，第二根手指唯一的合理語意
    /// 就是「不是在畫畫」。`isMultipleTouchEnabled = false` 已經由 UIKit 保證這件事，
    /// 這裡是第二道防線——它同時讓「哪一根是主的」有明確答案。
    private weak var tracked: UITouch?

    var isTracking: Bool { tracked != nil }

    /// `touchesBegan`。回傳 `nil` 表示這一根要忽略（已經有 active touch）。
    func begin(_ touch: UITouch, in view: UIView) -> InputSample? {
        guard tracked == nil else { return nil }
        tracked = touch
        strokeStart = touch.timestamp
        real.removeAll(keepingCapacity: true)
        predicted.removeAll(keepingCapacity: true)
        return sample(from: touch, in: view, predicted: false)
    }

    /// `touchesMoved`。**順序是真實在前、預測在後**，且預測點永遠在該批的尾端
    /// ——`stroke` 依賴這個順序做弧長取樣（§4）。
    func append(_ touch: UITouch, event: UIEvent?, in view: UIView) {
        guard touch == tracked else { return }

        for t in event?.coalescedTouches(for: touch) ?? [touch] {
            real.append(sample(from: t, in: view, predicted: false))
        }

        predicted.removeAll(keepingCapacity: true)
        for t in event?.predictedTouches(for: touch) ?? [] {
            predicted.append(sample(from: t, in: view, predicted: true))
        }
    }

    /// 取出這一 frame 的批次並清空。空陣列代表「不要呼叫 `appendSamples`」——
    /// 空的呼叫要跨一次 FFI，而它什麼也不做（§2.1）。
    func flush() -> [InputSample] {
        guard !real.isEmpty || !predicted.isEmpty else { return [] }
        let batch = real + predicted
        real.removeAll(keepingCapacity: true)
        predicted.removeAll(keepingCapacity: true)
        return batch
    }

    /// `touchesEnded` 之後。剩餘樣本已由呼叫端 flush 出去。
    func end(_ touch: UITouch) -> Bool {
        guard touch == tracked else { return false }
        tracked = nil
        strokeStart = nil
        return true
    }

    /// `touchesCancelled`：**丟棄未送出的樣本**。取消不需要完整性（§7）。
    func cancel(_ touch: UITouch) -> Bool {
        guard touch == tracked else { return false }
        real.removeAll(keepingCapacity: true)
        predicted.removeAll(keepingCapacity: true)
        tracked = nil
        strokeStart = nil
        return true
    }

    /// `preciseLocation` 而非 `location`——Pencil 有次像素精度，`location` 會捨去（§5）。
    ///
    /// 座標乘 `contentsScale` 在這裡做：`InputSample` 與 `tap` 一律送**螢幕像素**
    /// （`E1-bucket §4.1`）。`contentsScale` 是 layer 的屬性，Rust 不該去猜它。
    private func sample(from touch: UITouch, in view: UIView, predicted: Bool) -> InputSample {
        let point = touch.preciseLocation(in: view)
        let scale = view.layer.contentsScale

        // `maximumPossibleForce == 0`（不支援 force 的裝置）也走 radius 分支：
        // 除以 0 會產生 `NaN`，而 `NaN` 會沿著 One-Euro 傳染整筆（§6）。
        let usesForce = touch.type == .pencil && touch.maximumPossibleForce > 0

        return InputSample(
            x: Float(point.x * scale),
            y: Float(point.y * scale),
            t: Float(touch.timestamp - (strokeStart ?? touch.timestamp)),
            pressure: usesForce ? Float(touch.force / touch.maximumPossibleForce) : 0,
            // **`radius` 是點，不是像素**——不乘 `contentsScale`。`core/stroke §5`
            // 的 `R_EPS = 4.0` 明文以點為單位，而自適應正規化的分母是絕對量，
            // 換單位不會被約掉。`radius == 0` 是主動寫入的「這是觸控筆」語意
            // （`E1-stroke §2.2`），不是缺值——Pencil 的 `majorRadius` 也有值。
            radius: usesForce ? 0 : Float(touch.majorRadius),
            // E1 不使用，如實填以免 E2 才發現沒接。
            tiltX: Float(touch.altitudeAngle),
            tiltY: Float(touch.azimuthAngle(in: view)),
            predicted: predicted
        )
    }
}
