//! GPU 生命週期（`docs/specs/E1-wgpu.md §2`／`§3`）。
//!
//! ```text
//! new()                  → 建 Instance（不碰 GPU，headless 可跑）
//! attach_surface(handle) → Adapter → Device → Queue → Surface → DocumentResources
//! detach_surface()       → 只丟 Surface
//! ```

use colorpack::ColorPack;

use crate::error::RenderError;
use crate::gpu::{Gpu, instance_descriptor};
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
    attached: Option<Attached>,
}

impl RenderContext {
    /// 不碰 GPU——`Engine::new` 要能在無 GPU 環境成功（`ffi-contract.md §3`）。
    pub fn new() -> Self {
        Self {
            instance: wgpu::Instance::new(instance_descriptor()),
            gpu: None,
            resources: None,
            attached: None,
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
        let target = wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(
            handle.layer_ptr as *mut std::ffi::c_void,
        );
        let surface = unsafe { self.instance.create_surface_unsafe(target) }
            .map_err(RenderError::CreateSurface)?;

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
