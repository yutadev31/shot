use crate::frame::Frame;

pub mod wlr_screencopy;

pub trait Backend {
    fn initialize() -> crate::Result<Self>
    where
        Self: Sized;
    fn capture_output(&mut self) -> crate::Result<Frame>;
}
