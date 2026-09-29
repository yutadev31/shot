use std::os::fd::AsFd;

use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, WEnum,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_buffer, wl_output, wl_registry, wl_shm, wl_shm_pool},
};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1, zwlr_screencopy_manager_v1,
};

use crate::{
    backend::Backend,
    frame::{Frame, PixelFormat},
};

pub struct WlrScreencopyBackend {
    state: State,
    connection: Connection,
    event_queue: EventQueue<State>,

    shm: wl_shm::WlShm,
    output: wl_output::WlOutput,
    manager: zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
}

impl Backend for WlrScreencopyBackend {
    fn initialize() -> crate::Result<Self> {
        let connection = Connection::connect_to_env()?;
        let (globals, mut event_queue) = registry_queue_init::<State>(&connection)?;

        let qh = event_queue.handle();

        let shm = globals.bind::<wl_shm::WlShm, _, _>(&qh, 1..=1, ())?;

        let output = globals.bind::<wl_output::WlOutput, _, _>(&qh, 1..=4, ())?;

        let manager = globals.bind::<zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1, _, _>(
            &qh,
            1..=3,
            (),
        )?;

        let mut state = State::default();

        event_queue.blocking_dispatch(&mut state)?;

        Ok(Self {
            state,
            connection,
            event_queue,
            shm,
            output,
            manager,
        })
    }

    fn capture_output(&mut self) -> crate::Result<Frame> {
        let qh = self.event_queue.handle();

        self.state.buffer_info = None;
        self.state.ready = false;
        self.state.failed = false;

        let frame = self.manager.capture_output(0, &self.output, &qh, ());

        while self.state.buffer_info.is_none() && !self.state.failed {
            self.event_queue.blocking_dispatch(&mut self.state)?;
        }

        if self.state.failed {
            return Err(crate::Error::WaylandScreencopyFailed);
        }

        let info = self.state.buffer_info.take().unwrap();

        let size = info.stride as usize * info.height as usize;

        let file = tempfile::tempfile()?;
        file.set_len(size as u64)?;

        let mmap = unsafe { memmap2::MmapMut::map_mut(&file)? };

        let pool = self.shm.create_pool(file.as_fd(), size as i32, &qh, ());

        let buffer = pool.create_buffer(
            0,
            info.width as i32,
            info.height as i32,
            info.stride as i32,
            info.format,
            &qh,
            (),
        );

        frame.copy(&buffer);

        while !self.state.ready && !self.state.failed {
            self.event_queue.blocking_dispatch(&mut self.state)?;
        }

        if self.state.failed {
            return Err(crate::Error::WaylandScreencopyFailed);
        }

        let data = mmap.to_vec();
        let data = convert_to_rgba(&data, info.width, info.height, info.stride, info.format)?;
        let data = remove_stride(&data, info.width, info.height, info.stride);

        Ok(Frame {
            data,
            width: info.width,
            height: info.height,
            format: PixelFormat::Rgb8,
        })
    }
}

#[derive(Default)]
struct State {
    buffer_info: Option<BufferInfo>,
    ready: bool,
    failed: bool,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1, ()> for State {
    fn event(
        state: &mut Self,
        _frame: &zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1,
        event: zwlr_screencopy_frame_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_screencopy_frame_v1::Event::Buffer {
                format: WEnum::Value(format),
                width,
                height,
                stride,
            } => {
                state.buffer_info = Some(BufferInfo {
                    format,
                    width,
                    height,
                    stride,
                });
            }

            zwlr_screencopy_frame_v1::Event::Ready { .. } => {
                state.ready = true;
            }

            zwlr_screencopy_frame_v1::Event::Failed => {
                state.failed = true;
            }

            _ => {}
        }
    }
}

wayland_client::delegate_noop!(State: ignore wl_registry::WlRegistry);
wayland_client::delegate_noop!(State: ignore wl_output::WlOutput);
wayland_client::delegate_noop!(State: ignore wl_shm::WlShm);
wayland_client::delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(State: ignore wl_buffer::WlBuffer);
wayland_client::delegate_noop!(State: ignore zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1);

struct BufferInfo {
    format: wl_shm::Format,
    width: u32,
    height: u32,
    stride: u32,
}

fn remove_stride(data: &[u8], width: u32, height: u32, stride: u32) -> Vec<u8> {
    let row_size = width as usize * 4;
    let stride = stride as usize;

    let mut output = Vec::with_capacity(row_size * height as usize);

    for y in 0..height as usize {
        let start = y * stride;
        let end = start + row_size;

        output.extend_from_slice(&data[start..end]);
    }

    output
}

fn convert_to_rgba(
    data: &[u8],
    width: u32,
    height: u32,
    stride: u32,
    format: wl_shm::Format,
) -> crate::Result<Vec<u8>> {
    let width = width as usize;
    let height = height as usize;
    let stride = stride as usize;

    let mut output = Vec::with_capacity(width * height * 4);

    for y in 0..height {
        let row = &data[y * stride..y * stride + width * 4];

        for pixel in row.chunks_exact(4) {
            let b = pixel[0];
            let g = pixel[1];
            let r = pixel[2];

            match format {
                wl_shm::Format::Argb8888 => {
                    let a = pixel[3];
                    output.extend_from_slice(&[r, g, b, a]);
                }
                wl_shm::Format::Xrgb8888 => {
                    output.extend_from_slice(&[r, g, b, 255]);
                }
                _ => {
                    return Err(crate::Error::UnsupportedWaylandPixelFormat(format));
                }
            }
        }
    }

    Ok(output)
}
