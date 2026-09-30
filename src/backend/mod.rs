use crate::frame::Frame;

pub mod wayland;
pub mod x11;

pub trait Backend {
    fn initialize(monitor_index: usize) -> crate::Result<Self>
    where
        Self: Sized;
    fn initialize_all() -> crate::Result<Self>
    where
        Self: Sized;
    fn capture_output(&mut self) -> crate::Result<Frame>;
    fn capture_all_outputs(&mut self) -> crate::Result<Frame>;
    fn monitor_count(&self) -> usize;
    fn monitor_names(&self) -> Vec<String>;
    fn select_monitor(&mut self, monitor_index: usize) -> crate::Result<()>;
}
