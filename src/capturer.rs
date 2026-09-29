use std::{
    borrow::Cow,
    env, fs,
    io::{self, Cursor},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use arboard::{Clipboard, ImageData};
use image::{
    ExtendedColorType, ImageEncoder,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};

use crate::backend::{Backend, wlr_screencopy::WlrScreencopyBackend, x11::X11Backend};

pub struct Capturer {
    backend: Box<dyn Backend>,
}

impl Capturer {
    pub fn new() -> crate::Result<Self> {
        Self::new_for_monitor(0)
    }

    pub fn new_for_monitor(monitor_index: usize) -> crate::Result<Self> {
        let backend: Box<dyn Backend> = if env::var_os("WAYLAND_DISPLAY").is_some() {
            Box::new(WlrScreencopyBackend::initialize(monitor_index)?)
        } else if env::var_os("DISPLAY").is_some() {
            Box::new(X11Backend::initialize(monitor_index)?)
        } else {
            return Err(crate::Error::X11(
                "neither WAYLAND_DISPLAY nor DISPLAY is set".to_string(),
            ));
        };

        Ok(Self { backend })
    }

    pub fn capture_output(&mut self) -> crate::Result<()> {
        let frame = self.backend.capture_output()?;
        let png = encode_png(&frame)?;

        let path = save_to_file(&png)?;
        copy_to_clipboard(&png)?;
        println!("Saved screenshot to {}", path.display());
        println!("Copied screenshot to clipboard");

        Ok(())
    }
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

fn save_to_file(png: &[u8]) -> crate::Result<PathBuf> {
    let home = env::var_os("HOME").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "HOME environment variable is not set",
        )
    })?;
    let directory = PathBuf::from(home).join("Pictures").join("Screenshots");
    fs::create_dir_all(&directory)?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| io::Error::other(format!("system clock is before Unix epoch: {error}")))?
        .as_secs();
    let path = directory.join(format!("screenshot-{timestamp}.png"));
    fs::write(&path, png)?;

    Ok(path)
}

fn copy_to_clipboard(png: &[u8]) -> crate::Result<()> {
    let image = image::load_from_memory(png)
        .map_err(|_| crate::Error::ClipboardFailed)?
        .into_rgba8();
    let (width, height) = image.dimensions();
    let image = ImageData {
        width: width as usize,
        height: height as usize,
        bytes: Cow::Owned(image.into_raw()),
    };

    Clipboard::new()
        .map_err(|_| crate::Error::ClipboardFailed)?
        .set_image(image)
        .map_err(|_| crate::Error::ClipboardFailed)?;

    Ok(())
}
