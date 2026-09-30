use std::{
    borrow::Cow,
    env, fs,
    io::{self, Cursor},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(target_os = "linux")]
use std::process::{Command, Stdio};

use arboard::{Clipboard, ImageData};
use image::{
    ExtendedColorType, ImageEncoder,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};

use crate::backend::{Backend, wayland::WaylandBackend, x11::X11Backend};

pub struct Capturer {
    backend: Box<dyn Backend>,
}

impl Capturer {
    pub fn new() -> crate::Result<Self> {
        Self::new_for_monitor(0)
    }

    pub fn new_for_monitor(monitor_index: usize) -> crate::Result<Self> {
        let backend: Box<dyn Backend> = if env::var_os("WAYLAND_DISPLAY").is_some() {
            Box::new(WaylandBackend::initialize(monitor_index)?)
        } else if env::var_os("DISPLAY").is_some() {
            Box::new(X11Backend::initialize(monitor_index)?)
        } else {
            return Err(crate::Error::X11(
                "neither WAYLAND_DISPLAY nor DISPLAY is set".to_string(),
            ));
        };

        Ok(Self { backend })
    }

    pub fn new_for_monitor_name(name: &str) -> crate::Result<Self> {
        let mut capturer = Self::new_for_monitor_selection()?;
        let monitor_index = capturer
            .monitor_names()
            .iter()
            .position(|monitor_name| monitor_name == name)
            .ok_or_else(|| crate::Error::MonitorNotFound {
                name: name.to_string(),
                available: capturer.monitor_names(),
            })?;
        capturer.select_monitor(monitor_index)?;
        Ok(capturer)
    }

    pub fn new_for_monitor_selection() -> crate::Result<Self> {
        let backend: Box<dyn Backend> = if env::var_os("WAYLAND_DISPLAY").is_some() {
            Box::new(WaylandBackend::initialize_all()?)
        } else if env::var_os("DISPLAY").is_some() {
            Box::new(X11Backend::initialize_all()?)
        } else {
            return Err(crate::Error::X11(
                "neither WAYLAND_DISPLAY nor DISPLAY is set".to_string(),
            ));
        };

        Ok(Self { backend })
    }

    pub fn monitor_count(&self) -> usize {
        self.backend.monitor_count()
    }

    pub fn monitor_names(&self) -> Vec<String> {
        self.backend.monitor_names()
    }

    pub fn select_monitor(&mut self, monitor_index: usize) -> crate::Result<()> {
        self.backend.select_monitor(monitor_index)
    }

    pub fn new_for_all_monitors() -> crate::Result<Self> {
        let backend: Box<dyn Backend> = if env::var_os("WAYLAND_DISPLAY").is_some() {
            Box::new(WaylandBackend::initialize_all()?)
        } else if env::var_os("DISPLAY").is_some() {
            Box::new(X11Backend::initialize_all()?)
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
        copy_to_clipboard(&path)?;
        println!("Saved screenshot to {}", path.display());
        println!("Copied screenshot to clipboard");

        Ok(())
    }

    pub fn capture_all_outputs(&mut self) -> crate::Result<()> {
        let frame = self.backend.capture_all_outputs()?;
        let png = encode_png(&frame)?;

        let path = save_to_file(&png)?;
        copy_to_clipboard(&path)?;
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

#[cfg(target_os = "linux")]
fn copy_to_clipboard(path: &Path) -> crate::Result<()> {
    let executable = env::current_exe()?;
    Command::new(executable)
        .arg("--clipboard-daemon")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .current_dir("/")
        .spawn()?;

    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn copy_to_clipboard(path: &Path) -> crate::Result<()> {
    let png = fs::read(path)?;
    set_clipboard_image(&png)
}

pub fn serve_clipboard(path: &Path) -> crate::Result<()> {
    let png = fs::read(path)?;
    let image = image::load_from_memory(&png)
        .map_err(|_| crate::Error::ClipboardFailed)?
        .into_rgba8();
    let (width, height) = image.dimensions();
    let image = ImageData {
        width: width as usize,
        height: height as usize,
        bytes: Cow::Owned(image.into_raw()),
    };

    #[cfg(target_os = "linux")]
    {
        use arboard::SetExtLinux;

        let mut clipboard = Clipboard::new().map_err(|_| crate::Error::ClipboardFailed)?;
        clipboard
            .set()
            .wait()
            .image(image)
            .map_err(|_| crate::Error::ClipboardFailed)?;
    }

    #[cfg(not(target_os = "linux"))]
    {
        Clipboard::new()
            .map_err(|_| crate::Error::ClipboardFailed)?
            .set_image(image)
            .map_err(|_| crate::Error::ClipboardFailed)?;
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn set_clipboard_image(png: &[u8]) -> crate::Result<()> {
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
