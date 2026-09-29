use crate::frame::Frame;

pub mod wlr_screencopy;
pub mod x11;

pub trait Backend {
    fn initialize(monitor_index: usize) -> crate::Result<Self>
    where
        Self: Sized;
    fn capture_output(&mut self) -> crate::Result<Frame>;
}
