use std::io;

use image::{
    ExtendedColorType, ImageEncoder,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};

use crate::backend::{Backend, wlr_screencopy::WlrScreencopyBackend};

pub struct Capturer {
    backend: Box<dyn Backend>,
}

impl Capturer {
    pub fn new() -> crate::Result<Self> {
        Self::new_for_monitor(0)
    }

    pub fn new_for_monitor(monitor_index: usize) -> crate::Result<Self> {
        let backend = Box::new(WlrScreencopyBackend::initialize(monitor_index)?);

        Ok(Self { backend })
    }

    pub fn capture_output(&mut self) -> crate::Result<()> {
        let frame = self.backend.capture_output()?;

        let stdout = io::stdout();
        let writer = io::BufWriter::new(stdout.lock());

        PngEncoder::new_with_quality(writer, CompressionType::Fast, FilterType::Sub).write_image(
            &frame.data,
            frame.width,
            frame.height,
            ExtendedColorType::Rgba8,
        )?;

        Ok(())
    }
}
