//! wgpu 的 instance／adapter／device／queue（`docs/specs/E1-wgpu.md §2.1`）。

use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};

use crate::error::RenderError;

/// `E1-wgpu.md §2.1`：唯一的 required limit。畫布最大邊 2048（`Aspect::Portrait`）。
pub const MIN_TEXTURE_DIMENSION_2D: u32 = 2048;

/// device ＋ queue 的持有者。一次 `attach_surface` 建立，`detach` 不丟（C5）。
#[derive(Debug)]
pub struct Gpu {
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl Gpu {
    /// 不建 surface 的建立路徑——單元測試與 CI 用。
    pub fn headless() -> Result<Self, RenderError> {
        let instance = wgpu::Instance::new(instance_descriptor());
        Self::request(&instance, None)
    }

    /// `compatible_surface` 只影響 adapter 挑選，不改變 device 的能力。
    pub(crate) fn request(
        instance: &wgpu::Instance,
        compatible_surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Self, RenderError> {
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface,
            ..Default::default()
        }))
        .map_err(RenderError::NoAdapter)?;

        let limits = adapter.limits();
        if limits.max_texture_dimension_2d < MIN_TEXTURE_DIMENSION_2D {
            return Err(RenderError::TextureDimensionTooSmall {
                found: limits.max_texture_dimension_2d,
                required: MIN_TEXTURE_DIMENSION_2D,
            });
        }

        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("colorlull"),
            required_features: wgpu::Features::empty(),
            required_limits: limits,
            ..Default::default()
        }))
        .map_err(RenderError::NoDevice)?;

        Ok(Self {
            adapter,
            device,
            queue,
        })
    }

    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }
}

/// backend 限 Metal（§2.1）。v1 只有 iOS，多開 backend 只會讓失敗模式變多。
pub(crate) fn instance_descriptor() -> wgpu::InstanceDescriptor {
    wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    }
}

/// wgpu 的 native 路徑本來就是同步的，只是包成 `Future`。
/// 為此拉一個 async runtime 進來不划算，park／unpark 就夠。
fn block_on<F: Future>(future: F) -> F::Output {
    struct ThreadWaker(Thread);
    impl Wake for ThreadWaker {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }

    let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut future = pin!(future);

    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => thread::park(),
        }
    }
}
