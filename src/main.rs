use anyhow::Context;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition},
    error::OsError,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::macos::WindowAttributesExtMacOS,
    window::{Window, WindowId},
};

#[derive(Default)]
struct Giza {
    window: Option<Window>,
    error: Option<OsError>,
}

impl Giza {
    fn request_redraw(&self) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
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

        match event_loop.create_window(window_attributes) {
            Ok(window) => {
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
                self.window = Some(window);
                self.request_redraw();
            }
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
            }
        }
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
    }
}

fn main() -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    let mut giza = Giza::default();

    event_loop.run_app(&mut giza)?;

    if let Some(error) = giza.error.take() {
        return Err(error).context("failed to create window");
    }

    Ok(())
}
