//! 擴散動畫：buffer 佈局（`docs/specs/E1-composite.md §5`）與 CPU 推進（`E1-bucket §7`）。
//!
//! **per-tap 而非 per-region**：`origin` 與 `max_radius` 每次 tap 都重寫。
//!
//! 動畫狀態住在 `render` 不住在 `document`——時間進 `document` 會讓 `apply`
//! 從「狀態轉移」變成「狀態轉移 ＋ 副作用排程」（§7.1）。

use bytemuck::{Pod, Zeroable};

use crate::gpu::Gpu;
use crate::resources::DocumentResources;

/// 32 bytes／筆。65535 區上限 = 2 MB。
///
/// 對齊：`prev_color` 是 `vec4<f32>`，要 16-byte 對齊，所以它落在 offset 16，
/// 前面剛好塞得下 `origin`(8) ＋ `max_radius`(4) ＋ `progress`(4)。
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct FillAnim {
    /// 點擊處，畫布像素座標。
    pub origin: [f32; 2],
    /// origin 到 bbox 四角的**最大距離**——不是對角線（§7.4）。
    pub max_radius: f32,
    /// 0..1 的 eased 值，CPU 每 frame 更新。到 1 之後停止寫入。
    pub progress: f32,
    /// 這次填色之前該區域的顏色（編碼值 RGBA，straight alpha）。
    pub prev_color: [f32; 4],
}

pub const FILL_ANIM_SIZE: u64 = size_of::<FillAnim>() as u64;

impl Default for FillAnim {
    fn default() -> Self {
        Self::zeroed()
    }
}

const _: () = assert!(FILL_ANIM_SIZE == 32);

/// 擴散時長，秒（§7.3）。**初值**，實機調校列入 `E1-perf`。
pub const FILL_DURATION: f32 = 0.180;

/// ease-out cubic：`p = 1 - (1 - t)³`。**必須與 §7.5 的 `prev_color` 用同一條曲線**，
/// 否則連點時 CPU 算出來的起點與畫面上的顏色對不上，會跳變。
pub fn ease_out(t: f32) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}

/// `[u8; 4]` 編碼值 → shader 用的 `[f32; 4]`。**不 linearize**（`E1-wgpu §6`）。
pub fn encode(rgba: [u8; 4]) -> [f32; 4] {
    rgba.map(|c| f32::from(c) / 255.0)
}

/// 一次 tap 的擴散參數。`document::Effect::Filled` 的 GPU 側對應物。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fill {
    pub region_id: u32,
    /// 點擊處，**畫布**像素座標。
    pub origin: [f32; 2],
    /// `regions[region_id].bbox`，`[x, y, w, h]`。
    pub bbox: [u32; 4],
    /// 這次填色的顏色，編碼值。
    pub color: [f32; 4],
    /// 填色之前的顏色。該區還在動畫中的話會被 [`FillAnimator::begin`] 覆寫（§7.5）。
    pub prev: [f32; 4],
}

struct Active {
    region_id: u32,
    elapsed: f32,
    /// 這一筆的目標色，也就是 `Buf_palette[region_id]`。連點時是下一筆的插值終點（§7.5）。
    color: [f32; 4],
    anim: FillAnim,
}

/// 進行中的擴散動畫。動畫結束就從 `active` 移除——`Buf_fill` 的那一筆停在
/// `progress == 1`，之後永遠算出目標色。
#[derive(Default)]
pub struct FillAnimator {
    active: Vec<Active>,
}

impl FillAnimator {
    pub fn new() -> Self {
        Self::default()
    }

    /// 起一筆新的擴散。`prev` 是 `document` 交出來的舊 palette 值。
    ///
    /// 該區還在動畫中的話 `prev` 被**此刻畫面上的顏色**取代（§7.5）——CPU 與 shader
    /// 用同一條 `mix`，所以接得上，不跳變。
    pub fn begin(&mut self, gpu: &Gpu, res: &DocumentResources, fill: Fill) {
        let Fill {
            region_id,
            origin,
            bbox,
            color,
            prev,
        } = fill;

        let prev = match self.active.iter().position(|a| a.region_id == region_id) {
            Some(i) => {
                let old = self.active.swap_remove(i);
                let t = ease_out(old.elapsed / FILL_DURATION);
                mix(old.anim.prev_color, old.color, t)
            }
            None => prev,
        };

        let anim = FillAnim {
            origin,
            // origin 與 max_radius 一律用**新這次**的 tap 重算——per-tap 而非 per-region。
            max_radius: max_radius(origin, bbox),
            progress: 0.0,
            prev_color: prev,
        };
        res.write_fill(gpu, region_id, anim);

        self.active.push(Active {
            region_id,
            elapsed: 0.0,
            color,
            anim,
        });
    }

    /// 每 frame 推進一次。成本 = 進行中筆數 × 32 bytes `write_buffer`。
    ///
    /// 多筆同時進行不特別處理：180 ms 內人手點得出來的筆數是個位數，
    /// 不值得為它做批次上傳。
    pub fn advance(&mut self, gpu: &Gpu, res: &DocumentResources, dt: f32) {
        self.active.retain_mut(|a| {
            a.elapsed += dt;
            let progress = ease_out(a.elapsed / FILL_DURATION);
            a.anim.progress = progress;
            res.write_fill(gpu, a.region_id, a.anim);
            // 寫完 progress == 1 那一筆才移除，否則畫面停在最後一個未滿的值。
            a.elapsed < FILL_DURATION
        });
    }

    /// FrameDriver 用來判斷還需不需要繼續出 frame（`E1-input`）。
    pub fn is_animating(&self) -> bool {
        !self.active.is_empty()
    }
}

/// origin 到 bbox 四個角的最大距離。
///
/// **不是 bbox 對角線**——那個值在 origin 靠近某個角落時不夠大，擴散會在動畫結束時
/// 仍未覆蓋對角的另一端，視覺上是「填到一半就停了」（§7.4）。
fn max_radius(origin: [f32; 2], bbox: [u32; 4]) -> f32 {
    let [x, y, w, h] = bbox.map(|v| v as f32);
    [(x, y), (x + w, y), (x, y + h), (x + w, y + h)]
        .into_iter()
        .map(|(cx, cy)| (cx - origin[0]).hypot(cy - origin[1]))
        .fold(0.0, f32::max)
}

fn mix(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}
