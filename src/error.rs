#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unsupported Wayland pixel format: {0:?}")]
    UnsupportedWaylandPixelFormat(wayland_client::protocol::wl_shm::Format),

    #[error("Wayland screencopy capture failed")]
    WaylandScreencopyFailed,

    #[error("failed to read or write data: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to connect to Wayland compositor: {0}")]
    WaylandConnect(#[from] wayland_client::ConnectError),

    #[error("failed to dispatch Wayland events: {0}")]
    WaylandDispatch(#[from] wayland_client::DispatchError),

    #[error("failed to initialize Wayland globals: {0}")]
    WaylandGlobal(#[from] wayland_client::globals::GlobalError),

    #[error("failed to bind Wayland global: {0}")]
    WaylandBind(#[from] wayland_client::globals::BindError),

    #[error("monitor index {index} is out of range (found {count} monitor(s))")]
    MonitorOutOfRange { index: usize, count: usize },

    #[error("no monitors are available")]
    NoMonitors,

    #[error("monitor {name:?} was not found (available monitors: {available:?})")]
    MonitorNotFound {
        name: String,
        available: Vec<String>,
    },

    #[error("invalid image buffer")]
    InvalidImageBuffer,

    #[error("capture region is outside the selected monitor or has invalid dimensions")]
    InvalidRegion,

    #[error("failed to encode image: {0}")]
    Image(#[from] image::ImageError),

    #[error("unsupported path format: {0}")]
    InvalidPathFormat(String),

    #[error("no output destination is configured")]
    NoOutputDestination,

    #[error("display server is unavailable: {0}")]
    DisplayUnavailable(String),

    #[error("failed to connect to X11: {0}")]
    X11Connect(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("X11 screen is unavailable: {0}")]
    X11Screen(String),

    #[error("X11 visual is unavailable: {0}")]
    X11Visual(String),

    #[error("X11 image operation failed: {0}")]
    X11Image(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("refusing to write PNG data to a terminal; redirect stdout to a file or pipe")]
    StdoutIsTerminal,
}

pub type Result<T> = std::result::Result<T, Error>;
