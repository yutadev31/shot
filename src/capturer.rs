use std::io;
use std::io::{Cursor, IsTerminal, Write};

#[cfg(target_os = "linux")]
use std::env;

use image::{
    ExtendedColorType, ImageEncoder,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};

#[cfg(target_os = "linux")]
use crate::backend::wayland::WaylandBackend;
#[cfg(target_os = "linux")]
use crate::backend::x11::X11Backend;
use crate::backend::{Backend, BackendTarget, Frame};

/// Captures monitor images to standard output as PNG without explicit initialization.
///
/// ```no_run
/// use shot::capturer::Capturer;
///
/// Capturer::capture_output(0)?;
/// Capturer::capture_output_by_name("HDMI-A-1")?;
/// Capturer::capture_all_outputs()?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct Capturer;

struct CaptureSession {
    backend: Box<dyn Backend>,
    region: Option<(u32, u32, u32, u32)>,
}

impl CaptureSession {
    /// Creates a capturer for the zero-based monitor index.
    ///
    /// # Errors
    /// Returns an error if backend initialization fails or the index is out of range.
    fn new_for_monitor(monitor_index: usize) -> crate::Result<Self> {
        Self::with_backend(BackendTarget::Monitor(monitor_index))
    }

    /// Creates a capturer for an exact monitor name.
    ///
    /// # Errors
    /// Returns an error if backend initialization fails or the named monitor is absent.
    fn new_for_monitor_name(name: &str) -> crate::Result<Self> {
        let mut capturer = Self::new_for_monitor_selection()?;
        let names = capturer.backend.monitor_names();
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
    fn new_for_monitor_selection() -> crate::Result<Self> {
        Self::with_backend(BackendTarget::All)
    }

    /// Selects a monitor by its zero-based index.
    ///
    /// # Errors
    /// Returns an error if the index is out of range.
    fn select_monitor(&mut self, monitor_index: usize) -> crate::Result<()> {
        self.backend.select_monitor(monitor_index)
    }

    /// Creates a capturer that stitches all monitor images.
    ///
    /// # Errors
    /// Returns an error if the display backend cannot initialize or has no monitors.
    fn new_for_all_monitors() -> crate::Result<Self> {
        Self::with_backend(BackendTarget::All)
    }

    /// Captures the selected output, saves configured files, and optionally copies it.
    ///
    /// # Errors
    /// Returns an error if capture, cropping, encoding, saving, or clipboard access fails.
    fn capture_output(&mut self) -> crate::Result<()> {
        self.capture(|backend| backend.capture_output())
    }

    /// Captures all outputs as one image and sends it to configured destinations.
    ///
    /// # Errors
    /// Returns an error if capture, stitching, cropping, encoding, saving, or clipboard access fails.
    fn capture_all_outputs(&mut self) -> crate::Result<()> {
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
    fn set_region(&mut self, region: (u32, u32, u32, u32)) -> crate::Result<()> {
        if region.2 == 0 || region.3 == 0 {
            return Err(crate::Error::InvalidRegion);
        }

        self.region = Some(region);
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn create_backend(target: BackendTarget) -> crate::Result<Box<dyn Backend>> {
        if env::var_os("WAYLAND_DISPLAY").is_some() {
            return Ok(Box::new(WaylandBackend::initialize(target)?));
        }

        if env::var_os("DISPLAY").is_some() {
            return Ok(Box::new(X11Backend::initialize(target)?));
        }

        Err(crate::Error::DisplayUnavailable(
            "neither WAYLAND_DISPLAY nor DISPLAY is set".to_string(),
        ))
    }

    #[cfg(windows)]
    fn create_backend(target: BackendTarget) -> crate::Result<Box<dyn Backend>> {
        Ok(Box::new(
            crate::backend::windows::WindowsBackend::initialize(target)?,
        ))
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    fn create_backend(_target: BackendTarget) -> crate::Result<Box<dyn Backend>> {
        Err(crate::Error::DisplayUnavailable(
            "unsupported operating system".to_string(),
        ))
    }

    fn capture<F>(&mut self, capture: F) -> crate::Result<()>
    where
        F: FnOnce(&mut dyn Backend) -> crate::Result<Frame>,
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

fn crop_frame(frame: &Frame, x: u32, y: u32, width: u32, height: u32) -> crate::Result<Frame> {
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

    Ok(Frame {
        data,
        width,
        height,
    })
}

fn encode_png(frame: &Frame) -> crate::Result<Vec<u8>> {
    let mut png = Cursor::new(Vec::new());

    PngEncoder::new_with_quality(&mut png, CompressionType::Fast, FilterType::Sub).write_image(
        &frame.data,
        frame.width,
        frame.height,
        ExtendedColorType::Rgba8,
    )?;

    Ok(png.into_inner())
}

impl Capturer {
    /// Captures the zero-based monitor index to standard output as PNG.
    ///
    /// # Errors
    /// Returns an error if initialization, capture, encoding, or writing fails.
    pub fn capture_output(monitor_index: usize) -> crate::Result<()> {
        CaptureSession::new_for_monitor(monitor_index)?.capture_output()
    }

    /// Captures an exact monitor name to standard output as PNG.
    ///
    /// # Errors
    /// Returns an error if the monitor is absent or capture fails.
    pub fn capture_output_by_name(name: &str) -> crate::Result<()> {
        CaptureSession::new_for_monitor_name(name)?.capture_output()
    }

    /// Captures all outputs as one PNG image to standard output.
    ///
    /// # Errors
    /// Returns an error if initialization, capture, stitching, encoding, or writing fails.
    pub fn capture_all_outputs() -> crate::Result<()> {
        CaptureSession::new_for_all_monitors()?.capture_all_outputs()
    }

    /// Captures a region `(x, y, width, height)` within the selected monitor.
    ///
    /// # Errors
    /// Returns an error if the region is invalid or capture fails.
    pub fn capture_region(monitor_index: usize, region: (u32, u32, u32, u32)) -> crate::Result<()> {
        let mut session = CaptureSession::new_for_monitor(monitor_index)?;
        session.set_region(region)?;
        session.capture_output()
    }

    /// Returns the number of monitors advertised by the backend.
    ///
    /// # Errors
    /// Returns an error if backend initialization fails.
    pub fn monitor_count() -> crate::Result<usize> {
        Ok(CaptureSession::create_backend(BackendTarget::All)?.monitor_count())
    }

    /// Returns monitor names in the same order as their indices.
    ///
    /// # Errors
    /// Returns an error if backend initialization fails.
    pub fn monitor_names() -> crate::Result<Vec<String>> {
        Ok(CaptureSession::create_backend(BackendTarget::All)?.monitor_names())
    }
}
