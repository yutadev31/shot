use x11rb::{
    connection::Connection,
    image::Image,
    protocol::{
        randr::ConnectionExt as _,
        xproto::{ConnectionExt as _, ImageFormat},
    },
    rust_connection::RustConnection,
};

use crate::{
    backend::Backend,
    frame::{Frame, PixelFormat},
};

pub struct X11Backend {
    connection: RustConnection,
    root: u32,
    monitors: Vec<Monitor>,
    red_mask: u32,
    green_mask: u32,
    blue_mask: u32,
}

#[derive(Clone, Copy)]
struct Monitor {
    x: i16,
    y: i16,
    width: u16,
    height: u16,
}

impl Backend for X11Backend {
    fn initialize(monitor_index: usize) -> crate::Result<Self> {
        Self::initialize_with_monitors(Some(monitor_index))
    }

    fn initialize_all() -> crate::Result<Self> {
        Self::initialize_with_monitors(None)
    }

    fn capture_all_outputs(&mut self) -> crate::Result<Frame> {
        let monitors = self.monitors.clone();
        let mut frames = Vec::with_capacity(monitors.len());
        for monitor in monitors {
            frames.push(self.capture_monitor(monitor)?);
        }
        crate::frame::stitch_horizontally(&frames)
    }

    fn capture_output(&mut self) -> crate::Result<Frame> {
        self.capture_monitor(self.monitors[0])
    }
}

impl X11Backend {
    fn initialize_with_monitors(monitor_index: Option<usize>) -> crate::Result<Self> {
        let (connection, screen_num) =
            x11rb::connect(None).map_err(|error| crate::Error::X11(error.to_string()))?;
        let setup = connection.setup();
        let screen = setup
            .roots
            .get(screen_num)
            .ok_or_else(|| crate::Error::X11("X11 screen is unavailable".to_string()))?;
        let visual = screen
            .allowed_depths
            .iter()
            .flat_map(|depth| depth.visuals.iter())
            .find(|visual| visual.visual_id == screen.root_visual)
            .ok_or_else(|| crate::Error::X11("root visual is unavailable".to_string()))?;

        let monitors = connection
            .randr_get_monitors(screen.root, true)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| {
                reply
                    .monitors
                    .into_iter()
                    .map(|monitor| Monitor {
                        x: monitor.x,
                        y: monitor.y,
                        width: monitor.width,
                        height: monitor.height,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| {
                vec![Monitor {
                    x: 0,
                    y: 0,
                    width: screen.width_in_pixels,
                    height: screen.height_in_pixels,
                }]
            });
        let monitors = match monitor_index {
            Some(index) => vec![*monitors.get(index).ok_or(crate::Error::MonitorOutOfRange {
                index,
                count: monitors.len(),
            })?],
            None => monitors,
        };
        let root = screen.root;
        let red_mask = visual.red_mask;
        let green_mask = visual.green_mask;
        let blue_mask = visual.blue_mask;

        Ok(Self {
            connection,
            root,
            monitors,
            red_mask,
            green_mask,
            blue_mask,
        })
    }

    fn capture_monitor(&mut self, monitor: Monitor) -> crate::Result<Frame> {
        let reply = self
            .connection
            .get_image(
                ImageFormat::Z_PIXMAP,
                self.root,
                monitor.x,
                monitor.y,
                monitor.width,
                monitor.height,
                u32::MAX,
            )
            .map_err(|error| crate::Error::X11(error.to_string()))?
            .reply()
            .map_err(|error| crate::Error::X11(error.to_string()))?;
        let image = Image::get_from_reply(
            self.connection.setup(),
            monitor.width,
            monitor.height,
            reply,
        )
        .map_err(|error| crate::Error::X11(error.to_string()))?;
        let capacity = monitor.width as usize * monitor.height as usize * 4;
        let mut data = Vec::with_capacity(capacity);
        for y in 0..monitor.height {
            for x in 0..monitor.width {
                let pixel = image.get_pixel(x, y);
                data.extend_from_slice(&[
                    channel(pixel, self.red_mask),
                    channel(pixel, self.green_mask),
                    channel(pixel, self.blue_mask),
                    255,
                ]);
            }
        }
        Ok(Frame {
            data,
            width: monitor.width as u32,
            height: monitor.height as u32,
            format: PixelFormat::Rgba8,
        })
    }
}

fn channel(pixel: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let value = (pixel & mask) >> shift;
    let max = mask >> shift;
    ((value * 255 + max / 2) / max) as u8
}
