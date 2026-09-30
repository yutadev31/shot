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

    #[error("monitor {name:?} was not found (available monitors: {available:?})")]
    MonitorNotFound {
        name: String,
        available: Vec<String>,
    },

    #[error("invalid image buffer")]
    InvalidImageBuffer,

    #[error("failed to encode image: {0}")]
    Image(#[from] image::ImageError),

    #[error("failed to access clipboard")]
    ClipboardFailed,

    #[error("X11 operation failed: {0}")]
    X11(String),
}

pub type Result<T> = std::result::Result<T, Error>;
