//! GPU 生命週期（`docs/specs/E1-wgpu.md §2`／`§3`）。
//!
//! ```text
//! new()                  → 建 Instance（不碰 GPU，headless 可跑）
//! attach_surface(handle) → Adapter → Device → Queue → Surface → DocumentResources
//! detach_surface()       → 只丟 Surface
//! ```

use std::time::Instant;

use colorpack::ColorPack;
use stroke::Dab;

use crate::bounds::Bounds;
use crate::commit::CommitPass;
use crate::composite::{CompositePass, Frame};
use crate::dab::StrokePass;
use crate::erase::ErasePass;
use crate::error::RenderError;
use crate::fill::{Fill, FillAnimator, encode};
use crate::gpu::{Gpu, instance_descriptor};
use crate::mask::{MaskBinding, MaskUniform};
use crate::resources::DocumentResources;

/// Swift 端交出的唯一一個指標（`core/engine/src/ffi.rs` 的同名型別，S0 已定）。
///
/// **Boundary 1 紅線 1**：Native 交出 `layer_ptr` 之後一律不碰，drawable 由 wgpu 全權管理。
#[derive(Debug, Clone, Copy)]
pub struct SurfaceHandle {
    pub layer_ptr: u64,
    pub width_px: u32,
    pub height_px: u32,
    pub scale: f32,
}

/// `Bgra8Unorm` 而非 sRGB 變體——composite 直接輸出編碼值（§6）。
pub const SURFACE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

struct Attached {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}

pub struct RenderContext {
    instance: wgpu::Instance,
    gpu: Option<Gpu>,
    resources: Option<DocumentResources>,
    /// Pass 2 與 Pass 3 共用，所以住在 context 不住在任一個 pass（`E1-wgpu §7.1`）。
    mask: Option<MaskBinding>,
    composite: Option<CompositePass>,
    erase: Option<ErasePass>,
    stroke: Option<StrokePass>,
    commit: Option<CommitPass>,
    attached: Option<Attached>,
    /// 擴散動畫的 CPU 側（`E1-bucket §7`）。`document` 對它一無所知。
    fill_anim: FillAnimator,
    /// `render` 沒有時間參數也不擴 FFI（§7.2），dt 由這裡的兩次呼叫差算出。
    last_frame: Option<Instant>,
}

impl RenderContext {
    /// 不碰 GPU——`Engine::new` 要能在無 GPU 環境成功（`ffi-contract.md §3`）。
    pub fn new() -> Self {
        Self {
            instance: wgpu::Instance::new(instance_descriptor()),
            gpu: None,
            resources: None,
            mask: None,
            composite: None,
            erase: None,
            stroke: None,
            commit: None,
            attached: None,
            fill_anim: FillAnimator::new(),
            last_frame: None,
        }
    }

    pub fn prepare_document(&mut self, pack: &ColorPack) -> Result<(), RenderError> {
        if self.gpu.is_none() {
            let compatible = self.attached.as_ref().map(|a| &a.surface);
            self.gpu = Some(Gpu::request(&self.instance, compatible)?);
        }
        let gpu = self.gpu.as_ref().expect("剛剛才建好");

        if self.resources.is_none() {
            self.resources = Some(DocumentResources::new(gpu, pack)?);
        }
        let resources = self.resources.as_ref().expect("剛剛才建好");

        let mask = self.mask.get_or_insert_with(|| MaskBinding::new(gpu));
        // Pipeline 只建一次——mask mode 切換不重建（§6），資源格式不隨文件變。
        let composite = self
            .composite
            .get_or_insert_with(|| CompositePass::new(gpu, SURFACE_FORMAT, mask));
        composite.bind_document(gpu, resources);

        let erase = self.erase.get_or_insert_with(|| ErasePass::new(gpu));
        erase.bind_document(gpu, resources);

        let stroke = self.stroke.get_or_insert_with(|| StrokePass::new(gpu));
        stroke.bind_document(gpu, resources);

        let commit = self
            .commit
            .get_or_insert_with(|| CommitPass::new(gpu, mask));
        commit.bind_document(gpu, resources);
        Ok(())
    }

    /// **GPU 初始化的唯一時機**（§2）。S0 永遠回 `Ok`，E1 起真的會失敗（§2.2）。
    ///
    /// # Safety
    ///
    /// `handle.layer_ptr` 必須是一個活著的 `CAMetalLayer`，且在 `detach_surface`
    /// 之前不得釋放。
    pub unsafe fn attach_surface(
        &mut self,
        handle: SurfaceHandle,
        pack: &ColorPack,
    ) -> Result<(), RenderError> {
        let surface = unsafe { self.create_metal_surface(&handle) }?;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: SURFACE_FORMAT,
            // 非 sRGB 格式 ＋ Auto：presentation engine 把寫進去的值直接當編碼值看，
            // 硬體不做任何 decode／encode（§6）。
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: handle.width_px,
            height: handle.height_px,
            // `CADisplayLink` 驅動，vsync 對齊（§3.1）。FrameDriver 歸 `E1-input`。
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            // `maximumDrawableCount` 本身要在 Swift 端的 layer 上設，wgpu 不暴露它（§3.1）。
            desired_maximum_frame_latency: 2,
        };
        self.attached = Some(Attached { surface, config });

        self.prepare_document(pack)?;
        self.configure();
        Ok(())
    }

    /// `SurfaceTargetUnsafe::CoreAnimationLayer` 是 wgpu 的 `#[cfg(metal)]` variant，
    /// 非 Apple 平台上根本不存在。CI 的 rust job 跑在 Linux（`cargo build --workspace`），
    /// 所以這條路徑必須 cfg 分岔，否則整個 workspace 編不過。
    ///
    /// # Safety
    ///
    /// 同 [`RenderContext::attach_surface`]：`handle.layer_ptr` 必須是活著的 `CAMetalLayer`。
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    unsafe fn create_metal_surface(
        &self,
        handle: &SurfaceHandle,
    ) -> Result<wgpu::Surface<'static>, RenderError> {
        let target = wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(
            handle.layer_ptr as *mut std::ffi::c_void,
        );
        unsafe { self.instance.create_surface_unsafe(target) }.map_err(RenderError::CreateSurface)
    }

    /// 非 Apple 平台只為了編得過而存在——`render` 的其餘部分（含 golden test）仍照常在 CI 上跑。
    ///
    /// # Safety
    ///
    /// 沒有安全需求，簽名只是為了與 Apple 版本一致。
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    unsafe fn create_metal_surface(
        &self,
        _handle: &SurfaceHandle,
    ) -> Result<wgpu::Surface<'static>, RenderError> {
        Err(RenderError::UnsupportedPlatform)
    }

    /// 只重設 surface configuration——畫布尺寸來自 `manifest.canvas_size`，與螢幕無關。
    pub fn resize_surface(&mut self, width_px: u32, height_px: u32) {
        if let Some(attached) = self.attached.as_mut() {
            attached.config.width = width_px;
            attached.config.height = height_px;
        }
        self.configure();
    }

    /// **只丟 surface。** 丟了 device 或資源的話，使用者切出 App 再回來畫作就消失（C5）。
    pub fn detach_surface(&mut self) {
        self.attached = None;
    }

    /// 把 `document::Effect::Filled` 翻譯成 GPU 動作（`E1-bucket §3` 的第二段箭頭）。
    ///
    /// 三件事一起發生，缺一個都會在畫面上看得出來：寫 `Buf_palette`、清該區的
    /// `T_erase`（§6）、起一筆擴散動畫（§7）。`origin` 是 tap 的**畫布**像素座標。
    ///
    /// `document` 不呼叫這個函式——`deps-policy.toml` 禁止它看見 `render`，
    /// 中間永遠隔著 `engine`。
    pub fn fill(
        &mut self,
        region_id: u32,
        color: [u8; 4],
        prev: [u8; 4],
        bbox: [u32; 4],
        origin: [f32; 2],
    ) {
        let (Some(gpu), Some(res), Some(erase)) = (
            self.gpu.as_ref(),
            self.resources.as_ref(),
            self.erase.as_ref(),
        ) else {
            return;
        };

        // alpha 恆為 1.0：填色即不透明，`a == 0` 只用來表示「從未填過」（§5）。
        let color = encode(color);
        res.write_palette(gpu, region_id, color);
        erase.clear_region(gpu, res, region_id, bbox);
        self.fill_anim.begin(
            gpu,
            res,
            Fill {
                region_id,
                origin,
                bbox,
                color,
                prev: encode(prev),
            },
        );
    }

    /// Pass 1：把本 frame 新增的 dab 畫進 `T_wet`（`E1-stroke §7`）。
    ///
    /// 回傳這一批的包絡，呼叫端據此累積整筆 bbox——Pass 2 要的是整筆的，
    /// 而 scissor 用的是這一批的，兩者刻意不同。
    pub fn draw_dabs(&mut self, dabs: &[Dab], build_up: bool) -> Option<Bounds> {
        let (Some(gpu), Some(res), Some(stroke)) = (
            self.gpu.as_ref(),
            self.resources.as_ref(),
            self.stroke.as_ref(),
        ) else {
            return None;
        };
        stroke.draw(gpu, res, dabs, build_up);
        Bounds::of_dabs(dabs)
    }

    /// Pass 2：抬筆時一次，`T_wet × opacity × mask` → `T_paint`，收尾清 `T_wet`
    /// （`E1-stroke §8`）。
    ///
    /// `color` 是編碼值 straight alpha；整筆濃度由 `opacity` 決定，不看 `color.a`。
    pub fn commit_stroke(&mut self, color: [f32; 4], opacity: f32, bounds: Bounds) {
        let (Some(gpu), Some(res), Some(commit), Some(mask)) = (
            self.gpu.as_ref(),
            self.resources.as_ref(),
            self.commit.as_ref(),
            self.mask.as_ref(),
        ) else {
            return;
        };
        let Some(bbox) = bounds.to_scissor(res.canvas_size()) else {
            return;
        };
        commit.commit(gpu, res, mask, color, opacity, bbox);
    }

    /// 丟掉進行中的筆畫：只清 `T_wet`，**`T_paint` 從未被污染**。
    ///
    /// `cancel_stroke`（palm rejection 事後判定失敗）與 `end_stroke` 的重建
    /// （`E1-stroke §9`）走同一支。
    pub fn discard_wet(&mut self, bounds: Bounds) {
        let (Some(gpu), Some(res), Some(commit)) = (
            self.gpu.as_ref(),
            self.resources.as_ref(),
            self.commit.as_ref(),
        ) else {
            return;
        };
        let Some(bbox) = bounds.to_scissor(res.canvas_size()) else {
            return;
        };
        commit.clear_wet(gpu, res, bbox);
    }

    /// 還有擴散動畫在跑——FrameDriver 用它決定要不要繼續出 frame（`E1-input`）。
    pub fn is_animating(&self) -> bool {
        self.fill_anim.is_animating()
    }

    /// 一個 frame。dt 由內部的 `Instant` 取兩次呼叫的差（§7.2）——
    /// **不擴 FFI**，每 frame 都要傳的東西進 FFI 只會變成 Bridge 的另一個出錯點。
    pub fn render(&mut self, frame: Frame) -> Result<(), RenderError> {
        let now = Instant::now();
        let dt = self
            .last_frame
            .replace(now)
            .map_or(0.0, |prev| now.duration_since(prev).as_secs_f32());
        self.render_with_dt(frame, dt)
    }

    /// 一個 frame：推進動畫 ＋ Pass 3 Composite。Pass 1／2 由 `E1-stroke` 插在它前面。
    ///
    /// 沒有 surface（App 切到背景）時直接回 `Ok`——資源都還在，回前台就能繼續畫（C5）。
    ///
    /// 真正的實作吃 dt，`render` 是它的 wrapper：測試呼叫這一支，不需要 mock 框架。
    pub fn render_with_dt(&mut self, frame: Frame, dt: f32) -> Result<(), RenderError> {
        if let (Some(gpu), Some(res)) = (self.gpu.as_ref(), self.resources.as_ref()) {
            // 動畫先於 surface 檢查推進：背景時沒有 frame 會來，但一旦回前台，
            // 進行中的那幾筆不該從舊的 progress 重播。
            self.fill_anim.advance(gpu, res, dt);
        }

        let (Some(gpu), Some(composite), Some(mask), Some(attached)) = (
            self.gpu.as_ref(),
            self.composite.as_ref(),
            self.mask.as_ref(),
            self.attached.as_ref(),
        ) else {
            return Ok(());
        };

        composite.set_frame(gpu, frame);

        // 取不到 drawable 不是錯誤，是**掉一 frame**——下一次 `CADisplayLink` 再來。
        // `Outdated` / `Lost` 先重設 surface，其餘直接跳過。
        use wgpu::CurrentSurfaceTexture as Cst;
        let surface_texture = match attached.surface.get_current_texture() {
            Cst::Success(t) | Cst::Suboptimal(t) => t,
            Cst::Outdated | Cst::Lost => {
                self.configure();
                return Ok(());
            }
            Cst::Timeout | Cst::Occluded | Cst::Validation => return Ok(()),
        };
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        composite.draw(&mut encoder, &view, mask);
        gpu.queue().submit([encoder.finish()]);
        gpu.queue().present(surface_texture);
        Ok(())
    }

    /// 切 mask mode（D4 要能在真機上即時比較）：一次 `write_buffer`，不重建 pipeline。
    pub fn set_mask(&self, uniform: MaskUniform) {
        if let (Some(gpu), Some(mask)) = (self.gpu.as_ref(), self.mask.as_ref()) {
            mask.set(gpu, uniform);
        }
    }

    pub fn gpu(&self) -> Option<&Gpu> {
        self.gpu.as_ref()
    }

    pub fn resources(&self) -> Option<&DocumentResources> {
        self.resources.as_ref()
    }

    pub fn surface(&self) -> Option<&wgpu::Surface<'static>> {
        self.attached.as_ref().map(|a| &a.surface)
    }

    fn configure(&self) {
        if let (Some(gpu), Some(attached)) = (self.gpu.as_ref(), self.attached.as_ref()) {
            attached.surface.configure(gpu.device(), &attached.config);
        }
    }
}

impl Default for RenderContext {
    fn default() -> Self {
        Self::new()
    }
}
