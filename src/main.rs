use anyhow::Context;
use objc2::{MainThreadMarker, rc::Retained};
use objc2_app_kit::NSView;
use objc2_quartz_core::CAMetalLayer;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::macos::WindowAttributesExtMacOS,
    raw_window_handle::{AppKitWindowHandle, HasWindowHandle, RawWindowHandle},
    window::{Window, WindowId},
};

#[derive(Default)]
struct Giza {
    window: Option<Window>,
    metal_layer: Option<Retained<CAMetalLayer>>,
    error: Option<anyhow::Error>,
}

impl Giza {
    fn request_redraw(&self) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn retain_ns_view(window_handle: AppKitWindowHandle) -> anyhow::Result<Retained<NSView>> {
        let ns_view =
            // Safety: Raw AppKit handle provides a valid `NSView` pointer for the
            // lifetime of `window_handle` and retaining it here on the main thread is okay.
            unsafe { Retained::retain(window_handle.ns_view.as_ptr().cast::<NSView>()) }
                .context("window handle did not provide a valid NSView")?;

        Ok(ns_view)
    }

    fn retain_appkit_view(window: &Window) -> anyhow::Result<Retained<NSView>> {
        let _main_thread_marker =
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

        let ns_view = Self::retain_ns_view(app_kit_window_handle)?;

        Ok(ns_view)
    }

    fn attach_metal_layer(window: &Window) -> anyhow::Result<Retained<CAMetalLayer>> {
        let ns_view = Self::retain_appkit_view(window)?;
        let metal_layer = CAMetalLayer::new();

        ns_view.setWantsLayer(true);
        ns_view.setLayer(Some(metal_layer.as_ref()));

        Ok(metal_layer)
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
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.request_redraw();
            }
            _ => {}
        }
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.window = None;
        self.metal_layer = None;
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
