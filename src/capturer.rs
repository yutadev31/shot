use std::io::{Cursor, IsTerminal, Write};
use std::{env, io};

use image::{
    ExtendedColorType, ImageEncoder,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};

use crate::backend::wayland::WaylandBackend;
use crate::backend::x11::X11Backend;
use crate::backend::{Backend, BackendTarget};

pub struct Capturer {
    backend: Box<dyn Backend>,
    region: Option<(u32, u32, u32, u32)>,
}

impl Capturer {
    /// Creates a capturer for the first monitor.
    ///
    /// # Errors
    /// Returns an error if the display backend cannot connect or no monitor is available.
    pub fn new() -> crate::Result<Self> {
        Self::new_for_monitor(0)
    }

    /// Creates a capturer for the zero-based monitor index.
    ///
    /// # Errors
    /// Returns an error if backend initialization fails or the index is out of range.
    pub fn new_for_monitor(monitor_index: usize) -> crate::Result<Self> {
        Self::with_backend(BackendTarget::Monitor(monitor_index))
    }

    /// Creates a capturer for an exact monitor name.
    ///
    /// # Errors
    /// Returns an error if backend initialization fails or the named monitor is absent.
    pub fn new_for_monitor_name(name: &str) -> crate::Result<Self> {
        let mut capturer = Self::new_for_monitor_selection()?;
        let names = capturer.monitor_names();
        let monitor_index = names
            .iter()
            .position(|monitor_name| monitor_name == name)
            .ok_or_else(|| crate::Error::MonitorNotFound {
                name: name.to_string(),
                available: names,
            })?;
        capturer.select_monitor(monitor_index)?;
        Ok(capturer)
    }

    /// Creates a capturer with all monitors available for later selection.
    ///
    /// # Errors
    /// Returns an error if the display backend cannot initialize or has no monitors.
    pub fn new_for_monitor_selection() -> crate::Result<Self> {
        Self::with_backend(BackendTarget::All)
    }

    /// Returns the number of monitors advertised by the backend.
    #[must_use]
    pub fn monitor_count(&self) -> usize {
        self.backend.monitor_count()
    }

    /// Returns monitor names in the same order as their indices.
    #[must_use]
    pub fn monitor_names(&self) -> Vec<String> {
        self.backend.monitor_names()
    }

    /// Selects a monitor by its zero-based index.
    ///
    /// # Errors
    /// Returns an error if the index is out of range.
    pub fn select_monitor(&mut self, monitor_index: usize) -> crate::Result<()> {
        self.backend.select_monitor(monitor_index)
    }

    /// Creates a capturer that stitches all monitor images.
    ///
    /// # Errors
    /// Returns an error if the display backend cannot initialize or has no monitors.
    pub fn new_for_all_monitors() -> crate::Result<Self> {
        Self::with_backend(BackendTarget::All)
    }

    /// Captures the selected output, saves configured files, and optionally copies it.
    ///
    /// # Errors
    /// Returns an error if capture, cropping, encoding, saving, or clipboard access fails.
    pub fn capture_output(&mut self) -> crate::Result<()> {
        self.capture(|backend| backend.capture_output())
    }

    /// Captures all outputs as one image and sends it to configured destinations.
    ///
    /// # Errors
    /// Returns an error if capture, stitching, cropping, encoding, saving, or clipboard access fails.
    pub fn capture_all_outputs(&mut self) -> crate::Result<()> {
        self.capture(|backend| backend.capture_all_outputs())
    }

    fn with_backend(target: BackendTarget) -> crate::Result<Self> {
        Ok(Self {
            backend: Self::create_backend(target)?,
            region: None,
        })
    }

    /// Restrict the saved image to `(x, y, width, height)` pixels within the selected monitor.
    ///
    /// # Errors
    /// Returns an error if width or height is zero. Bounds are checked during capture.
    pub fn set_region(&mut self, region: (u32, u32, u32, u32)) -> crate::Result<()> {
        if region.2 == 0 || region.3 == 0 {
            return Err(crate::Error::InvalidRegion);
        }

        self.region = Some(region);
        Ok(())
    }

    fn create_backend(target: BackendTarget) -> crate::Result<Box<dyn Backend>> {
        if env::var_os("WAYLAND_DISPLAY").is_some() {
            return Ok(Box::new(WaylandBackend::initialize(target)?));
        }

        if env::var_os("DISPLAY").is_some() {
            return Ok(Box::new(X11Backend::initialize(target)?));
        }

        return Err(crate::Error::DisplayUnavailable(
            "neither WAYLAND_DISPLAY nor DISPLAY is set".to_string(),
        ));
    }

    fn capture<F>(&mut self, capture: F) -> crate::Result<()>
    where
        F: FnOnce(&mut dyn Backend) -> crate::Result<crate::frame::Frame>,
    {
        if io::stdout().is_terminal() {
            return Err(crate::Error::StdoutIsTerminal);
        }

        let mut frame = capture(self.backend.as_mut())?;

        if let Some((x, y, width, height)) = self.region {
            frame = crop_frame(&frame, x, y, width, height)?;
        }

        let png = encode_png(&frame)?;

        let mut stdout = io::stdout().lock();
        stdout.write_all(&png)?;

        Ok(())
    }
}

fn crop_frame(
    frame: &crate::frame::Frame,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> crate::Result<crate::frame::Frame> {
    if width == 0
        || height == 0
        || x.checked_add(width).is_none_or(|right| right > frame.width)
        || y.checked_add(height)
            .is_none_or(|bottom| bottom > frame.height)
    {
        return Err(crate::Error::InvalidRegion);
    }

    let source_stride = (frame.width as usize)
        .checked_mul(4)
        .ok_or(crate::Error::InvalidImageBuffer)?;
    let output_stride = (width as usize)
        .checked_mul(4)
        .ok_or(crate::Error::InvalidImageBuffer)?;
    let capacity = output_stride
        .checked_mul(height as usize)
        .ok_or(crate::Error::InvalidImageBuffer)?;

    if frame.data.len()
        != source_stride
            .checked_mul(frame.height as usize)
            .ok_or(crate::Error::InvalidImageBuffer)?
    {
        return Err(crate::Error::InvalidImageBuffer);
    }

    let mut data = Vec::with_capacity(capacity);

    for row in y..y + height {
        let start = row as usize * source_stride + x as usize * 4;
        data.extend_from_slice(&frame.data[start..start + output_stride]);
    }

    Ok(crate::frame::Frame {
        data,
        width,
        height,
    })
}

fn encode_png(frame: &crate::frame::Frame) -> crate::Result<Vec<u8>> {
    let mut png = Cursor::new(Vec::new());

    PngEncoder::new_with_quality(&mut png, CompressionType::Fast, FilterType::Sub).write_image(
        &frame.data,
        frame.width,
        frame.height,
        ExtendedColorType::Rgba8,
    )?;

    Ok(png.into_inner())
}
