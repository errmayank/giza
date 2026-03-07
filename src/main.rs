use anyhow::Context;
use objc2::{MainThreadMarker, rc::Retained, runtime::ProtocolObject};
use objc2_app_kit::NSView;
use objc2_core_foundation::CGSize;
use objc2_metal::{
    MTLClearColor, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue,
    MTLCreateSystemDefaultDevice, MTLDevice, MTLLoadAction, MTLPixelFormat,
    MTLRenderPassDescriptor, MTLStoreAction,
};
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::macos::WindowAttributesExtMacOS,
    raw_window_handle::{AppKitWindowHandle, HasWindowHandle, RawWindowHandle},
    window::{Window, WindowId},
};

struct MetalState {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    command_queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
}

#[derive(Default)]
struct Giza {
    window: Option<Window>,
    metal_layer: Option<Retained<CAMetalLayer>>,
    metal_state: Option<MetalState>,
    error: Option<anyhow::Error>,
}

impl Giza {
    fn request_redraw(&self) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn retain_ns_view(
        _: MainThreadMarker,
        window_handle: AppKitWindowHandle,
    ) -> anyhow::Result<Retained<NSView>> {
        let ns_view =
            // Safety: Raw AppKit handle provides a valid `NSView` pointer for the
            // lifetime of `window_handle` and retaining it here on the main thread is okay.
            unsafe { Retained::retain(window_handle.ns_view.as_ptr().cast::<NSView>()) }
                .context("window handle did not provide a valid NSView")?;

        Ok(ns_view)
    }

    fn retain_appkit_view(window: &Window) -> anyhow::Result<Retained<NSView>> {
        let main_thread_marker =
            MainThreadMarker::new().context("AppKit view access must happen on the main thread")?;
        let window_handle = window
            .window_handle()
            .context("failed to access window handle")?;
        let app_kit_window_handle = match window_handle.as_raw() {
            RawWindowHandle::AppKit(handle) => handle,
            _ => {
                return Err(anyhow::anyhow!(
                    "expected an AppKit window handle for the macOS window"
                ));
            }
        };

        let ns_view = Self::retain_ns_view(main_thread_marker, app_kit_window_handle)?;

        Ok(ns_view)
    }

    fn attach_metal_layer(window: &Window) -> anyhow::Result<Retained<CAMetalLayer>> {
        let ns_view = Self::retain_appkit_view(window)?;
        let metal_layer = CAMetalLayer::new();

        ns_view.setWantsLayer(true);
        ns_view.setLayer(Some(metal_layer.as_ref()));

        Ok(metal_layer)
    }

    fn create_metal_state() -> anyhow::Result<MetalState> {
        let device =
            MTLCreateSystemDefaultDevice().context("failed to get the default Metal device")?;
        let command_queue = device
            .newCommandQueue()
            .context("failed to create Metal command queue")?;

        Ok(MetalState {
            device,
            command_queue,
        })
    }

    fn update_metal_layer_size(window: &Window, metal_layer: &CAMetalLayer) {
        let window_size = window.inner_size();
        let scale_factor = window.scale_factor();

        metal_layer.setContentsScale(scale_factor);
        metal_layer.setDrawableSize(CGSize {
            width: window_size.width as f64,
            height: window_size.height as f64,
        });
    }

    fn configure_metal_layer(
        window: &Window,
        metal_layer: &CAMetalLayer,
        metal_state: &MetalState,
    ) {
        metal_layer.setDevice(Some(metal_state.device.as_ref()));
        metal_layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
        metal_layer.setFramebufferOnly(true);
        metal_layer.setMaximumDrawableCount(3);
        metal_layer.setAllowsNextDrawableTimeout(false);

        Self::update_metal_layer_size(window, metal_layer);
    }

    fn draw(&mut self) -> anyhow::Result<()> {
        let metal_layer = self
            .metal_layer
            .as_ref()
            .context("missing CAMetalLayer during redraw")?;
        let metal_state = self
            .metal_state
            .as_ref()
            .context("missing Metal state during redraw")?;

        let Some(drawable) = metal_layer.nextDrawable() else {
            return Ok(());
        };

        let command_buffer = metal_state
            .command_queue
            .commandBuffer()
            .context("failed to create Metal command buffer")?;
        let render_pass_descriptor = MTLRenderPassDescriptor::new();

        let color_attachment =
            // Safety: `colorAttachments[0]` is the first valid color attachment slot in Metal.
            unsafe { render_pass_descriptor.colorAttachments().objectAtIndexedSubscript(0) };

        color_attachment.setTexture(Some(drawable.texture().as_ref()));
        color_attachment.setLoadAction(MTLLoadAction::Clear);
        color_attachment.setStoreAction(MTLStoreAction::Store);
        color_attachment.setClearColor(MTLClearColor {
            red: 20.0 / 255.0,
            green: 20.0 / 255.0,
            blue: 20.0 / 255.0,
            alpha: 1.0,
        });

        let command_encoder = command_buffer
            .renderCommandEncoderWithDescriptor(&render_pass_descriptor)
            .context("failed to create Metal render command encoder")?;

        command_encoder.endEncoding();
        command_buffer.presentDrawable(drawable.as_ref());
        command_buffer.commit();

        Ok(())
    }
}

impl ApplicationHandler for Giza {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_attributes = Window::default_attributes()
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true)
            .with_title_hidden(true)
            .with_inner_size(LogicalSize::new(960.0, 740.0));

        let window = match event_loop
            .create_window(window_attributes)
            .context("failed to create window")
        {
            Ok(window) => window,
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
                return;
            }
        };

        if let Some(monitor) = window.current_monitor() {
            let monitor_position = monitor.position();
            let monitor_size = monitor.size();
            let window_size = window.outer_size();
            let center_x = monitor_position.x
                + (monitor_size.width.saturating_sub(window_size.width) / 2) as i32;
            let center_y = monitor_position.y
                + (monitor_size.height.saturating_sub(window_size.height) / 2) as i32;

            window.set_outer_position(PhysicalPosition::new(center_x, center_y - 50));
        }

        let metal_layer =
            match Self::attach_metal_layer(&window).context("failed to attach CAMetalLayer") {
                Ok(metal_layer) => metal_layer,
                Err(error) => {
                    self.error = Some(error);
                    event_loop.exit();
                    return;
                }
            };

        let metal_state = match Self::create_metal_state().context("failed to initialize Metal") {
            Ok(metal_state) => metal_state,
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
                return;
            }
        };

        Self::configure_metal_layer(&window, metal_layer.as_ref(), &metal_state);

        self.metal_state = Some(metal_state);
        self.metal_layer = Some(metal_layer);
        self.window = Some(window);
        self.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.as_ref() else {
            return;
        };

        if window.id() != window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.draw().context("failed to render frame") {
                    self.error = Some(error);
                    event_loop.exit();
                }
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(metal_layer) = self.metal_layer.as_ref() {
                    Self::update_metal_layer_size(window, metal_layer.as_ref());
                }

                self.request_redraw();
            }
            _ => {}
        }
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.window = None;
        self.metal_layer = None;
        self.metal_state = None;
    }
}

fn main() -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    let mut giza = Giza::default();

    event_loop
        .run_app(&mut giza)
        .context("event loop exited with an error")?;

    if let Some(error) = giza.error.take() {
        return Err(error);
    }

    Ok(())
}
