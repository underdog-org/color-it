//
//  DesignTokens.swift
//  ColorApp
//
//  **來源：`design/mobile-design.pen` 的 document variables。**
//

import SwiftUI

/// 設計 token 的命名空間。刻意用 `enum` 而不是 `struct`——它永遠不該被實例化。
enum DS {

    // MARK: - 顏色

    enum Color {
        /// 畫面底色。
        static let bg = SwiftUI.Color(hex: 0xE7_E3_DD)
        /// 卡片、工具列膠囊等浮起面。
        static let surface = SwiftUI.Color(hex: 0xFF_FF_FF)
        /// 主前景色。當前色的選取環也用它。
        static let ink = SwiftUI.Color(hex: 0x16_13_0F)
        /// 次要文字、未選中的 icon。
        static let muted = SwiftUI.Color(hex: 0x8C_85_7C)
        /// 分隔線、骨架佔位。
        static let line = SwiftUI.Color(hex: 0xE3_DE_D6)
        /// 唯一的強調色：進度條填充、PAID 徽章、重試。
        static let accent = SwiftUI.Color(hex: 0xF4_61_4C)
        /// 紙張（畫布）底色。
        static let paper = SwiftUI.Color(hex: 0xFB_F8_F3)

        // 品牌色。v1 只當假資料縮圖的底色與示範色票用，不進語意角色。

        static let brandAmber = SwiftUI.Color(hex: 0xF2_A9_3B)
        static let brandTeal = SwiftUI.Color(hex: 0x3F_A8_9B)
        static let brandPeri = SwiftUI.Color(hex: 0x7B_85_E0)
        static let brandBlush = SwiftUI.Color(hex: 0xEE_9B_B4)
    }

    // MARK: - 間距

    enum Space {
        static let space1: CGFloat = 4
        static let space2: CGFloat = 8
        static let space3: CGFloat = 12
        static let space4: CGFloat = 16
        static let space5: CGFloat = 20
        static let space6: CGFloat = 24
    }

    // MARK: - 圓角

    enum Radius {
        static let sm: CGFloat = 6
        /// 卡片縮圖。`Card States · Work` 的進度條軌道左右內縮 12px 就是為了讓開這個曲率。
        static let md: CGFloat = 20
        static let lg: CGFloat = 32
        /// 膠囊。`.pen` 寫 999，Swift 這邊給 `Capsule()` 用，數值本身只在必須用
        /// `RoundedRectangle` 的場合出現。
        static let pill: CGFloat = 999
    }

    // MARK: - 字級與字型

    /// `.pen` 的字級是固定 px；iOS 這側一律走 `.custom(_:size:relativeTo:)`，
    /// 讓系統 Dynamic Type 縮放。`relativeTo` 挑的是**字級最接近的**內建 text style，
    /// 這樣縮放曲線才不會走樣。
    ///
    /// **大字級與日文的破版檢查不在 S1 驗收**（`S1-ios-ui.md §3`），留給 i18n 那一輪。
    enum Typography {
        static let fontDisplay = "Fraunces"
        static let fontBody = "Inter"

        static let sizeCaption: CGFloat = 11
        static let sizeBody: CGFloat = 13
        static let sizeTitle: CGFloat = 19
        static let sizeDisplay: CGFloat = 32

        /// 11pt：卡片副標、色票列標籤、狀態註記。
        static let caption = Font.custom(fontBody, size: sizeCaption, relativeTo: .caption2)
        /// 13pt：卡片標題、內文。
        static let body = Font.custom(fontBody, size: sizeBody, relativeTo: .footnote)
        /// 19pt：區塊標題。
        static let title = Font.custom(fontBody, size: sizeTitle, relativeTo: .title3)
        /// 32pt：畫面主標題，唯一使用 display 字族的地方。
        static let display = Font.custom(fontDisplay, size: sizeDisplay, relativeTo: .largeTitle)
    }
}

extension Color {
    /// `0xRRGGBB` 字面值。設計稿的顏色是 sRGB，`Color(.sRGB, …)` 也是——不需要色彩空間轉換。
    ///
    /// 只給這份 token 檔用；View 一律引 `DS.Color.*`，不自己寫 hex。
    fileprivate init(hex: UInt32) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255,
            opacity: 1
        )
    }
}
