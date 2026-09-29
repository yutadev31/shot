use crate::frame::Frame;

pub mod wayland;
pub mod x11;

pub trait Backend {
    fn initialize(monitor_index: usize) -> crate::Result<Self>
    where
        Self: Sized;
    fn capture_output(&mut self) -> crate::Result<Frame>;
}
