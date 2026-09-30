use std::{
    borrow::Cow,
    env, fs,
    io::Cursor,
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
        Self::with_backend(BackendSelection::Monitor(monitor_index))
    }

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

    pub fn new_for_monitor_selection() -> crate::Result<Self> {
        Self::with_backend(BackendSelection::All)
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
        Self::with_backend(BackendSelection::All)
    }

    pub fn capture_output(&mut self) -> crate::Result<()> {
        self.capture(|backend| backend.capture_output())
    }

    pub fn capture_all_outputs(&mut self) -> crate::Result<()> {
        self.capture(|backend| backend.capture_all_outputs())
    }

    fn with_backend(selection: BackendSelection) -> crate::Result<Self> {
        let backend: Box<dyn Backend> = if env::var_os("WAYLAND_DISPLAY").is_some() {
            match selection {
                BackendSelection::Monitor(index) => Box::new(WaylandBackend::initialize(index)?),
                BackendSelection::All => Box::new(WaylandBackend::initialize_all()?),
            }
        } else if env::var_os("DISPLAY").is_some() {
            match selection {
                BackendSelection::Monitor(index) => Box::new(X11Backend::initialize(index)?),
                BackendSelection::All => Box::new(X11Backend::initialize_all()?),
            }
        } else {
            return Err(crate::Error::DisplayUnavailable(
                "neither WAYLAND_DISPLAY nor DISPLAY is set".to_string(),
            ));
        };

        Ok(Self { backend })
    }

    fn capture<F>(&mut self, capture: F) -> crate::Result<()>
    where
        F: FnOnce(&mut dyn Backend) -> crate::Result<crate::frame::Frame>,
    {
        let frame = capture(self.backend.as_mut())?;
        let png = encode_png(&frame)?;

        let path = save_to_file(&png)?;
        copy_to_clipboard(&path)?;
        println!("Saved screenshot to {}", path.display());
        println!("Copied screenshot to clipboard");

        Ok(())
    }
}

enum BackendSelection {
    Monitor(usize),
    All,
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
        crate::Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "HOME environment variable is not set",
        ))
    })?;
    let directory = PathBuf::from(home).join("Pictures").join("Screenshots");
    fs::create_dir_all(&directory)?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            crate::Error::Io(std::io::Error::other(format!(
                "system clock is before Unix epoch: {error}"
            )))
        })?
        .as_millis();
    let mut path = directory.join(format!("screenshot-{timestamp}.png"));
    for suffix in 1.. {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => {
                use std::io::Write;
                let mut file = file;
                file.write_all(png)?;
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                path = directory.join(format!("screenshot-{timestamp}-{suffix}.png"));
            }
            Err(error) => return Err(error.into()),
        }
    }

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
    let image = image_data_from_png(&png)?;

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
    let image = image_data_from_png(png)?;

    Clipboard::new()
        .map_err(|_| crate::Error::ClipboardFailed)?
        .set_image(image)
        .map_err(|_| crate::Error::ClipboardFailed)?;

    Ok(())
}

fn image_data_from_png(png: &[u8]) -> crate::Result<ImageData<'static>> {
    let image = image::load_from_memory(png)
        .map_err(|_| crate::Error::ClipboardFailed)?
        .into_rgba8();
    let (width, height) = image.dimensions();
    Ok(ImageData {
        width: width as usize,
        height: height as usize,
        bytes: Cow::Owned(image.into_raw()),
    })
}
