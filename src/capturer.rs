use std::{
    borrow::Cow,
    env, fs,
    io::Cursor,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
use std::process::{Command, Stdio};

use arboard::{Clipboard, ImageData};
use image::{
    ExtendedColorType, ImageEncoder,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};

#[cfg(all(
    not(any(target_os = "windows", target_os = "macos")),
    feature = "wayland"
))]
use crate::backend::wayland::WaylandBackend;
#[cfg(all(target_os = "windows", feature = "windows"))]
use crate::backend::windows::WindowsBackend;
#[cfg(all(not(any(target_os = "windows", target_os = "macos")), feature = "x11"))]
use crate::backend::x11::X11Backend;
use crate::backend::{Backend, BackendTarget};

pub struct Capturer {
    backend: Box<dyn Backend>,
}

impl Capturer {
    pub fn new() -> crate::Result<Self> {
        Self::new_for_monitor(0)
    }

    pub fn new_for_monitor(monitor_index: usize) -> crate::Result<Self> {
        Self::with_backend(BackendTarget::Monitor(monitor_index))
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
        Self::with_backend(BackendTarget::All)
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
        Self::with_backend(BackendTarget::All)
    }

    pub fn capture_output(&mut self) -> crate::Result<()> {
        self.capture(|backend| backend.capture_output())
    }

    pub fn capture_all_outputs(&mut self) -> crate::Result<()> {
        self.capture(|backend| backend.capture_all_outputs())
    }

    fn with_backend(target: BackendTarget) -> crate::Result<Self> {
        Ok(Self {
            backend: Self::create_backend(target)?,
        })
    }

    fn create_backend(target: BackendTarget) -> crate::Result<Box<dyn Backend>> {
        #[cfg(all(
            not(any(target_os = "windows", target_os = "macos")),
            feature = "wayland"
        ))]
        if env::var_os("WAYLAND_DISPLAY").is_some() {
            return Ok(Box::new(WaylandBackend::initialize(target)?));
        }

        #[cfg(all(not(any(target_os = "windows", target_os = "macos")), feature = "x11"))]
        if env::var_os("DISPLAY").is_some() {
            return Ok(Box::new(X11Backend::initialize(target)?));
        }

        #[cfg(all(target_os = "windows", feature = "windows"))]
        return Ok(Box::new(WindowsBackend::initialize(target)?));

        #[cfg(all(
            not(any(target_os = "windows", target_os = "macos")),
            feature = "wayland",
            feature = "x11"
        ))]
        return Err(crate::Error::DisplayUnavailable(
            "neither WAYLAND_DISPLAY nor DISPLAY is set".to_string(),
        ));

        #[cfg(all(
            not(any(target_os = "windows", target_os = "macos")),
            feature = "wayland",
            not(feature = "x11")
        ))]
        return Err(crate::Error::DisplayUnavailable(
            "WAYLAND_DISPLAY is not set".to_string(),
        ));

        #[cfg(all(
            not(any(target_os = "windows", target_os = "macos")),
            not(feature = "wayland"),
            feature = "x11"
        ))]
        return Err(crate::Error::DisplayUnavailable(
            "DISPLAY is not set".to_string(),
        ));

        #[cfg(not(any(
            all(
                not(any(target_os = "windows", target_os = "macos")),
                any(feature = "wayland", feature = "x11")
            ),
            all(target_os = "windows", feature = "windows")
        )))]
        return Err(crate::Error::DisplayUnavailable(
            "no backend is enabled for this operating system".to_string(),
        ));
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
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .ok_or_else(|| {
            crate::Error::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "HOME or USERPROFILE environment variable is not set",
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

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
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

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn copy_to_clipboard(path: &Path) -> crate::Result<()> {
    let png = fs::read(path)?;
    set_clipboard_image(&png)
}

pub fn serve_clipboard(path: &Path) -> crate::Result<()> {
    let png = fs::read(path)?;
    let image = image_data_from_png(&png)?;

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        use arboard::SetExtLinux;

        let mut clipboard = Clipboard::new().map_err(|_| crate::Error::ClipboardFailed)?;
        clipboard
            .set()
            .wait()
            .image(image)
            .map_err(|_| crate::Error::ClipboardFailed)?;
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        Clipboard::new()
            .map_err(|_| crate::Error::ClipboardFailed)?
            .set_image(image)
            .map_err(|_| crate::Error::ClipboardFailed)?;
    }

    Ok(())
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
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
