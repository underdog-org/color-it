//
//  ColorWheel.swift
//  ColorApp
//
//  設計稿：`Draw · Tools v2` 的 `Color Dock`（170pt 高，色環大半沉在畫面外）。
//

import EngineBridge
import SwiftUI

struct ColorWheel: View {
    @Binding var color: Rgba
    let isPicking: Bool
    let onEyedropper: () -> Void

    private static let hueSteps = 18
    private static let outerBand = (saturation: 0.48, brightness: 0.765)
    private static let innerBand = (saturation: 0.58, brightness: 0.59)

    private static let outerDiameter: CGFloat = 420
    private static let innerDiameter: CGFloat = 360
    private static let discDiameter: CGFloat = 300
    private static let dockHeight: CGFloat = 170
    /// 環心相對於 dock 頂端的 y。設計稿是 `y: 16` ＋ 半徑。
    private static let centerY: CGFloat = 16 + outerDiameter / 2

    var body: some View {
        ZStack(alignment: .top) {
            wheel
            eyedropper
        }
        .frame(height: Self.dockHeight)
        .clipped()
    }

    // MARK: - 環

    private var wheel: some View {
        ZStack {
            band(diameter: Self.outerDiameter, thickness: 30, band: Self.outerBand)
            band(diameter: Self.innerDiameter, thickness: 31, band: Self.innerBand)
            disc
            knob
        }
        .frame(width: Self.outerDiameter, height: Self.outerDiameter)
        // 環中心沉到 dock 下方：往上對齊後再把中心推回設計稿的位置。
        .offset(y: Self.centerY - Self.outerDiameter / 2)
        .contentShape(Circle())
        .gesture(hueDrag)
    }

    private func band(
        diameter: CGFloat, thickness: CGFloat, band: (saturation: Double, brightness: Double)
    ) -> some View {
        Circle()
            .strokeBorder(
                AngularGradient(
                    stops: Self.stops(band: band),
                    center: .center,
                    // 0° 在正上方，順時針。`Angle` 的 0° 在右側，所以退 90°。
                    startAngle: .degrees(-90),
                    endAngle: .degrees(270)
                ),
                lineWidth: thickness
            )
            .frame(width: diameter, height: diameter)
    }

    /// 中心盤：當前色。半徑向外微微變亮，讓它看起來是一顆顏料而不是一塊色卡。
    private var disc: some View {
        let base = Color(color)
        return Circle()
            .fill(
                RadialGradient(
                    colors: [base.mix(with: .white, by: 0.22), base],
                    center: .center,
                    startRadius: 0,
                    endRadius: Self.discDiameter / 2
                )
            )
            .frame(width: Self.discDiameter, height: Self.discDiameter)
            .overlay(alignment: .top) {
                // 色名在盤上緣、剛好露出來的那一段。S1 只給假名字。
                Text(ColorNames.name(for: color))
                    .font(DS.Typography.body)
                    .fontWeight(.medium)
                    .foregroundStyle(.white.opacity(0.7))
                    .padding(.top, 36)
            }
    }

    /// 旋鈕壓在外圈上，位置由當前色的色相決定。
    private var knob: some View {
        let angle = Angle.degrees(Self.hue(of: color) * 360 - 90)
        let radius = (Self.outerDiameter - 30) / 2
        return Circle()
            .fill(Color(color))
            .frame(width: 56, height: 56)
            .overlay(Circle().strokeBorder(.white, lineWidth: 3))
            .offset(
                x: radius * CGFloat(cos(angle.radians)),
                y: radius * CGFloat(sin(angle.radians))
            )
    }

    // MARK: - 吸管

    private var eyedropper: some View {
        Button(action: onEyedropper) {
            Image(systemName: isPicking ? "eyedropper.halffull" : "eyedropper")
                .font(.system(size: 21))
                .foregroundStyle(isPicking ? DS.Color.accent : DS.Color.ink)
                .frame(width: 56, height: 56)
                .background(
                    Circle()
                        .fill(DS.Color.surface)
                        .shadow(color: DS.Color.ink.opacity(0.22), radius: 10, x: -7, y: 2)
                )
        }
        .buttonStyle(.plain)
        .accessibilityLabel("吸管取色")
        .accessibilityAddTraits(isPicking ? [.isSelected] : [])
        .frame(maxWidth: .infinity, alignment: .trailing)
        .padding(.trailing, -18)
        .padding(.top, 36)
    }

    // MARK: - 互動

    /// 拖曳環帶：角度決定色相，落點半徑決定亮帶或暗帶。
    private var hueDrag: some Gesture {
        DragGesture(minimumDistance: 0).onChanged { value in
            let center = CGPoint(x: Self.outerDiameter / 2, y: Self.outerDiameter / 2)
            let dx = value.location.x - center.x
            let dy = value.location.y - center.y
            let distance = (dx * dx + dy * dy).squareRoot()
            // 中心盤不參與選色——那是結果的顯示區。
            guard distance > Self.discDiameter / 2 else { return }

            let hue = (atan2(dy, dx) + .pi / 2) / (2 * .pi)
            let band = distance > (Self.innerDiameter / 2)
                ? Self.outerBand
                : Self.innerBand
            color = Self.rgba(
                hue: Double(hue < 0 ? hue + 1 : hue),
                saturation: band.saturation,
                brightness: band.brightness
            )
        }
    }

    // MARK: - 色相 ↔ Rgba

    private static func stops(
        band: (saturation: Double, brightness: Double)
    ) -> [Gradient.Stop] {
        (0...hueSteps).map { step in
            let hue = Double(step % hueSteps) / Double(hueSteps)
            return Gradient.Stop(
                color: Color(
                    hue: hue, saturation: band.saturation, brightness: band.brightness
                ),
                location: Double(step) / Double(hueSteps)
            )
        }
    }

    private static func rgba(hue: Double, saturation: Double, brightness: Double) -> Rgba {
        let (r, g, b) = hsbToRgb(hue: hue, saturation: saturation, brightness: brightness)
        return Rgba(
            r: UInt8((r * 255).rounded()),
            g: UInt8((g * 255).rounded()),
            b: UInt8((b * 255).rounded()),
            a: 0xFF
        )
    }

    /// 只給旋鈕定位用，所以不需要完整的 RGB→HSB，取色相就好。
    private static func hue(of rgba: Rgba) -> Double {
        let r = Double(rgba.r) / 255
        let g = Double(rgba.g) / 255
        let b = Double(rgba.b) / 255
        let maxValue = max(r, g, b)
        let minValue = min(r, g, b)
        let delta = maxValue - minValue
        guard delta > 0 else { return 0 }

        let hue: Double = switch maxValue {
        case r: (g - b) / delta / 6
        case g: (2 + (b - r) / delta) / 6
        default: (4 + (r - g) / delta) / 6
        }
        return hue < 0 ? hue + 1 : hue
    }

    private static func hsbToRgb(
        hue: Double, saturation: Double, brightness: Double
    ) -> (Double, Double, Double) {
        let sector = hue * 6
        let index = Int(sector) % 6
        let fraction = sector - Double(Int(sector))
        let p = brightness * (1 - saturation)
        let q = brightness * (1 - saturation * fraction)
        let t = brightness * (1 - saturation * (1 - fraction))

        return switch index {
        case 0: (brightness, t, p)
        case 1: (q, brightness, p)
        case 2: (p, brightness, t)
        case 3: (p, q, brightness)
        case 4: (t, p, brightness)
        default: (brightness, p, q)
        }
    }
}

/// 色名。S1 給的是**色相分箱的通俗名**，不是真的色彩命名系統——
/// 它只是讓中心盤不至於是一塊沒有說明的色塊。i18n 那一輪會換成字串表。
enum ColorNames {
    private static let names = [
        "朱紅", "陶土", "蜜糖", "蜂蜜", "青苔", "湖水",
        "薄荷", "淺灘", "陰天", "靛青", "紫藤", "櫻花",
    ]

    static func name(for rgba: Rgba) -> String {
        let r = Double(rgba.r) / 255
        let g = Double(rgba.g) / 255
        let b = Double(rgba.b) / 255
        let maxValue = max(r, g, b)
        let minValue = min(r, g, b)
        let delta = maxValue - minValue
        guard delta > 0.04 else { return maxValue > 0.5 ? "淺灰" : "炭黑" }

        let hue: Double = switch maxValue {
        case r: (g - b) / delta / 6
        case g: (2 + (b - r) / delta) / 6
        default: (4 + (r - g) / delta) / 6
        }
        let normalized = hue < 0 ? hue + 1 : hue
        return names[min(Int(normalized * Double(names.count)), names.count - 1)]
    }
}

#Preview("Color Dock") {
    @Previewable @State var color = Rgba(r: 0x6C, g: 0x92, b: 0xC6, a: 0xFF)

    VStack(spacing: 0) {
        Spacer()
        ColorWheel(color: $color, isPicking: false, onEyedropper: {})
    }
    .frame(width: 390, height: 300)
    .background(DS.Color.bg)
}
