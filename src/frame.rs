#[derive(Debug)]
pub struct Frame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Rgb8,
    Rgba8,
}

pub fn stitch_horizontally(frames: &[Frame]) -> crate::Result<Frame> {
    let height = frames.iter().map(|frame| frame.height).max().unwrap_or(0);
    let width = frames.iter().try_fold(0u32, |total, frame| {
        total
            .checked_add(frame.width)
            .ok_or(crate::Error::InvalidImageBuffer)
    })?;
    let row_bytes = (width as usize)
        .checked_mul(4)
        .ok_or(crate::Error::InvalidImageBuffer)?;
    let capacity = row_bytes
        .checked_mul(height as usize)
        .ok_or(crate::Error::InvalidImageBuffer)?;
    let mut data = vec![0; capacity];
    let mut x_offset = 0u32;

    for frame in frames {
        if frame.format != PixelFormat::Rgba8 {
            return Err(crate::Error::InvalidImageBuffer);
        }
        let source_row_bytes = (frame.width as usize)
            .checked_mul(4)
            .ok_or(crate::Error::InvalidImageBuffer)?;
        let expected_len = source_row_bytes
            .checked_mul(frame.height as usize)
            .ok_or(crate::Error::InvalidImageBuffer)?;
        if frame.data.len() != expected_len {
            return Err(crate::Error::InvalidImageBuffer);
        }

        for row in 0..frame.height as usize {
            let source_start = row * source_row_bytes;
            let destination_start = row * row_bytes + x_offset as usize * 4;
            data[destination_start..destination_start + source_row_bytes]
                .copy_from_slice(&frame.data[source_start..source_start + source_row_bytes]);
        }
        x_offset += frame.width;
    }

    Ok(Frame {
        data,
        width,
        height,
        format: PixelFormat::Rgba8,
    })
}
