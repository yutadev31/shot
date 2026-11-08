use std::{collections::HashMap, os::fd::AsFd};

use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, WEnum,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_buffer, wl_output, wl_registry, wl_shm, wl_shm_pool},
};
use wayland_protocols::ext::{
    image_capture_source::v1::client::{
        ext_image_capture_source_v1, ext_output_image_capture_source_manager_v1,
    },
    image_copy_capture::v1::client::{
        ext_image_copy_capture_frame_v1, ext_image_copy_capture_manager_v1,
        ext_image_copy_capture_session_v1,
    },
};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1, zwlr_screencopy_manager_v1,
};

use crate::backend::{Backend, BackendTarget, Frame, PositionedFrame, stitch_positioned};

pub struct WaylandBackend {
    state: State,
    event_queue: EventQueue<State>,

    shm: wl_shm::WlShm,
    outputs: Vec<Output>,
    names: Vec<String>,
    selected_monitor: Option<usize>,
    manager: CaptureManager,
}

impl Backend for WaylandBackend {
    fn initialize(target: BackendTarget) -> crate::Result<Self> {
        let monitor_index = match target {
            BackendTarget::Monitor(index) => Some(index),
            BackendTarget::All => None,
        };

        Self::initialize_with_outputs(monitor_index)
    }

    fn capture_all_outputs(&mut self) -> crate::Result<Frame> {
        let mut frames = Vec::with_capacity(self.outputs.len());

        let outputs = self.outputs.clone();
        let captured = self.capture_outputs(&outputs)?;

        for (output, frame) in outputs.iter().zip(captured) {
            let layout = self
                .state
                .output_layouts
                .get(&output.global_name)
                .copied()
                .ok_or(crate::Error::WaylandScreencopyFailed)?;
            frames.push(PositionedFrame {
                frame,
                x: layout.x,
                y: layout.y,
            });
        }

        stitch_positioned(&frames)
    }

    fn capture_output(&mut self) -> crate::Result<Frame> {
        let index = self.selected_monitor.unwrap_or(0);
        let output = self
            .outputs
            .get(index)
            .cloned()
            .ok_or(crate::Error::NoMonitors)?;
        self.capture_outputs(&[output])?
            .pop()
            .ok_or(crate::Error::WaylandScreencopyFailed)
    }

    fn monitor_count(&self) -> usize {
        self.outputs.len()
    }

    fn monitor_names(&self) -> Vec<String> {
        self.names.clone()
    }

    fn select_monitor(&mut self, monitor_index: usize) -> crate::Result<()> {
        if monitor_index >= self.outputs.len() {
            return Err(crate::Error::MonitorOutOfRange {
                index: monitor_index,
                count: self.outputs.len(),
            });
        }

        self.selected_monitor = Some(monitor_index);
        Ok(())
    }
}

impl WaylandBackend {
    fn initialize_with_outputs(monitor_index: Option<usize>) -> crate::Result<Self> {
        let connection = Connection::connect_to_env()?;
        let (globals, mut event_queue) = registry_queue_init::<State>(&connection)?;

        let qh = event_queue.handle();

        let shm = globals.bind::<wl_shm::WlShm, _, _>(&qh, 1..=1, ())?;

        let output_globals = globals.contents().with_list(|globals| {
            globals
                .iter()
                .filter(|global| global.interface == "wl_output")
                .map(|global| (global.name, global.version))
                .collect::<Vec<_>>()
        });
        let output_count = output_globals.len();

        if output_count == 0 {
            return Err(crate::Error::NoMonitors);
        }

        if let Some(index) = monitor_index
            && index >= output_count
        {
            return Err(crate::Error::MonitorOutOfRange {
                index,
                count: output_count,
            });
        }

        let selected_globals = output_globals.clone();
        let outputs = selected_globals
            .clone()
            .into_iter()
            .map(|(name, version)| Output {
                global_name: name,
                proxy: globals.registry().bind(name, version.min(4), &qh, name),
            })
            .collect();

        let has_ext = globals.contents().with_list(|globals| {
            [
                "ext_image_copy_capture_manager_v1",
                "ext_output_image_capture_source_manager_v1",
            ]
            .iter()
            .all(|interface| globals.iter().any(|global| global.interface == *interface))
        });
        let manager = if has_ext {
            CaptureManager::Ext {
                capture: globals.bind(&qh, 1..=1, ())?,
                source: globals.bind(&qh, 1..=1, ())?,
            }
        } else {
            CaptureManager::Wlr(globals.bind(&qh, 1..=3, ())?)
        };

        let mut state = State::default();

        event_queue.roundtrip(&mut state)?;

        let names = selected_globals
            .iter()
            .map(|(name, _)| {
                state
                    .output_names
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| format!("wl-output-{name}"))
            })
            .collect();

        Ok(Self {
            state,
            event_queue,
            shm,
            outputs,
            names,
            selected_monitor: monitor_index,
            manager,
        })
    }

    fn capture_outputs(&mut self, outputs: &[Output]) -> crate::Result<Vec<Frame>> {
        self.state.captures.clear();
        let qh = self.event_queue.handle();
        let mut captures = Vec::with_capacity(outputs.len());

        // Queue all requests on the same connection before dispatching events.
        for output in outputs {
            let id = self.state.next_capture_id;
            self.state.next_capture_id = id
                .checked_add(1)
                .ok_or(crate::Error::WaylandScreencopyFailed)?;
            self.state.captures.insert(id, CaptureState::default());

            let capture = match &self.manager {
                CaptureManager::Ext { capture, source } => {
                    let source = source.create_source(&output.proxy, &qh, ());
                    let session = capture.create_session(
                        &source,
                        ext_image_copy_capture_manager_v1::Options::empty(),
                        &qh,
                        id,
                    );
                    source.destroy();
                    ActiveCapture::Ext {
                        frame: session.create_frame(&qh, id),
                        session,
                    }
                }
                CaptureManager::Wlr(manager) => {
                    ActiveCapture::Wlr(manager.capture_output(0, &output.proxy, &qh, id))
                }
            };
            captures.push((id, capture));
        }

        let mut buffers = Vec::with_capacity(captures.len());

        for (id, capture) in &captures {
            while !self.state.captures[id].constraints_done && !self.state.captures[id].failed {
                self.event_queue.blocking_dispatch(&mut self.state)?;
            }

            let state = self.state.captures.get_mut(id).unwrap();
            state.check_error()?;
            let info = state
                .buffer_info
                .take()
                .ok_or(crate::Error::WaylandScreencopyFailed)?;
            let buffer = CaptureBuffer::new(&self.shm, &qh, info)?;

            match capture {
                ActiveCapture::Ext { frame, .. } => {
                    frame.attach_buffer(&buffer.proxy);
                    frame.damage_buffer(0, 0, buffer.info.width as i32, buffer.info.height as i32);
                    frame.capture();
                }
                ActiveCapture::Wlr(frame) => frame.copy(&buffer.proxy),
            }

            buffers.push(buffer);
        }

        let mut frames = Vec::with_capacity(captures.len());

        for ((id, _), buffer) in captures.iter().zip(&buffers) {
            while !self.state.captures[id].ready && !self.state.captures[id].failed {
                self.event_queue.blocking_dispatch(&mut self.state)?;
            }

            let state = &self.state.captures[id];
            state.check_error()?;
            let info = &buffer.info;
            let data = convert_to_rgba(
                &buffer.mmap,
                info.width,
                info.height,
                info.stride,
                info.format,
            )?;
            frames.push(transform_frame(
                Frame {
                    data,
                    width: info.width,
                    height: info.height,
                },
                state.transform,
            )?);
        }

        Ok(frames)
    }
}

struct CaptureBuffer {
    proxy: wl_buffer::WlBuffer,
    mmap: memmap2::MmapMut,
    info: BufferInfo,
}

impl CaptureBuffer {
    fn new(shm: &wl_shm::WlShm, qh: &QueueHandle<State>, info: BufferInfo) -> crate::Result<Self> {
        let row_size = info.width as usize * 4;
        let stride = info.stride as usize;

        if stride < row_size {
            return Err(crate::Error::InvalidImageBuffer);
        }

        let size = stride
            .checked_mul(info.height as usize)
            .ok_or(crate::Error::InvalidImageBuffer)?;
        let size_i32 = i32::try_from(size).map_err(|_| crate::Error::InvalidImageBuffer)?;
        let width = i32::try_from(info.width).map_err(|_| crate::Error::InvalidImageBuffer)?;
        let height = i32::try_from(info.height).map_err(|_| crate::Error::InvalidImageBuffer)?;
        let stride = i32::try_from(info.stride).map_err(|_| crate::Error::InvalidImageBuffer)?;

        let file = tempfile::tempfile()?;
        file.set_len(size as u64)?;

        // SAFETY: This private file has length `size` and is never truncated.
        // The compositor writes through the shared buffer before sending Ready.
        let mmap = unsafe { memmap2::MmapMut::map_mut(&file)? };
        let pool = shm.create_pool(file.as_fd(), size_i32, qh, ());

        let buffer = pool.create_buffer(0, width, height, stride, info.format, qh, ());

        pool.destroy();

        Ok(Self {
            proxy: buffer,
            mmap,
            info,
        })
    }
}

impl Drop for CaptureBuffer {
    fn drop(&mut self) {
        self.proxy.destroy();
    }
}

#[derive(Default)]
struct State {
    output_names: HashMap<u32, String>,
    output_layouts: HashMap<u32, OutputLayout>,
    captures: HashMap<usize, CaptureState>,
    next_capture_id: usize,
}

#[derive(Default)]
struct CaptureState {
    buffer_info: Option<BufferInfo>,
    constraints_done: bool,
    ext_size: Option<(u32, u32)>,
    ext_format: Option<wl_shm::Format>,
    transform: Option<wl_output::Transform>,
    ready: bool,
    failed: bool,
    unsupported_format: Option<wl_shm::Format>,
}

impl CaptureState {
    fn check_error(&self) -> crate::Result<()> {
        if let Some(format) = self.unsupported_format {
            return Err(crate::Error::UnsupportedWaylandPixelFormat(format));
        }

        if self.failed {
            return Err(crate::Error::WaylandScreencopyFailed);
        }

        Ok(())
    }
}

impl Dispatch<wl_output::WlOutput, u32> for State {
    fn event(
        state: &mut Self,
        _output: &wl_output::WlOutput,
        event: wl_output::Event,
        global_name: &u32,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_output::Event::Name { name } => {
                state.output_names.insert(*global_name, name);
            }
            wl_output::Event::Geometry { x, y, .. } => {
                state.output_layouts.entry(*global_name).or_default().x = x;

                state.output_layouts.entry(*global_name).or_default().y = y;
            }
            _ => {}
        }
    }
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

impl Dispatch<zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1, usize> for State {
    fn event(
        state: &mut Self,
        _frame: &zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1,
        event: zwlr_screencopy_frame_v1::Event,
        id: &usize,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let Some(state) = state.captures.get_mut(id) else {
            return;
        };

        match event {
            zwlr_screencopy_frame_v1::Event::Buffer {
                format: WEnum::Value(format),

                width,
                height,
                stride,
            } => {
                if matches!(format, wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888) {
                    state.constraints_done = true;
                    state.buffer_info = Some(BufferInfo {
                        format,
                        width,
                        height,
                        stride,
                    });
                } else {
                    state.unsupported_format = Some(format);
                    state.failed = true;
                }
            }
            zwlr_screencopy_frame_v1::Event::Buffer { .. } => {
                // Do not keep dispatching forever when the compositor advertises
                // an enum value this client does not know.
                state.failed = true;
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

#[derive(Clone, Copy, Default)]
struct OutputLayout {
    x: i32,
    y: i32,
}

#[derive(Clone)]
struct Output {
    global_name: u32,
    proxy: wl_output::WlOutput,
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

    let mut output = Vec::with_capacity(
        width
            .checked_mul(height)
            .and_then(|size| size.checked_mul(4))
            .ok_or(crate::Error::InvalidImageBuffer)?,
    );

    for y in 0..height {
        let start = y
            .checked_mul(stride)
            .ok_or(crate::Error::InvalidImageBuffer)?;
        let end = start
            .checked_add(
                width
                    .checked_mul(4)
                    .ok_or(crate::Error::InvalidImageBuffer)?,
            )
            .ok_or(crate::Error::InvalidImageBuffer)?;
        let row = data
            .get(start..end)
            .ok_or(crate::Error::InvalidImageBuffer)?;

        let (pixels, _) = row.as_chunks::<4>();

        for pixel in pixels {
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

enum CaptureManager {
    Ext {
        capture: ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1,
        source: ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
    },
    Wlr(zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1),
}

enum ActiveCapture {
    Ext {
        frame: ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1,
        session: ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1,
    },
    Wlr(zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1),
}

impl Drop for ActiveCapture {
    fn drop(&mut self) {
        match self {
            Self::Ext { frame, session } => {
                frame.destroy();
                session.destroy();
            }
            Self::Wlr(frame) => frame.destroy(),
        }
    }
}

impl Dispatch<ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1, usize> for State {
    fn event(
        state: &mut Self,
        _session: &ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1,
        event: ext_image_copy_capture_session_v1::Event,
        id: &usize,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let Some(state) = state.captures.get_mut(id) else {
            return;
        };

        use ext_image_copy_capture_session_v1::Event;
        match event {
            Event::BufferSize { width, height } => state.ext_size = Some((width, height)),
            Event::ShmFormat {
                format: WEnum::Value(format),
            } => {
                if matches!(format, wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888) {
                    state.ext_format = Some(format);
                } else {
                    state.unsupported_format = Some(format);
                }
            }
            Event::Done => {
                state.constraints_done = true;
                if let (Some((width, height)), Some(format)) = (state.ext_size, state.ext_format)
                    && width > 0
                    && height > 0
                    && let Some(stride) = width.checked_mul(4)
                {
                    state.unsupported_format = None;
                    state.buffer_info = Some(BufferInfo {
                        format,
                        width,
                        height,
                        stride,
                    });
                } else {
                    state.failed = true;
                }
            }
            Event::Stopped => state.failed = true,
            _ => {}
        }
    }
}

impl Dispatch<ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1, usize> for State {
    fn event(
        state: &mut Self,
        _frame: &ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1,
        event: ext_image_copy_capture_frame_v1::Event,
        id: &usize,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let Some(state) = state.captures.get_mut(id) else {
            return;
        };

        use ext_image_copy_capture_frame_v1::Event;
        match event {
            Event::Transform {
                transform: WEnum::Value(transform),
            } => state.transform = Some(transform),
            Event::Transform { .. } | Event::Failed { .. } => state.failed = true,
            Event::Ready => state.ready = true,
            _ => {}
        }
    }
}

wayland_client::delegate_noop!(State: ignore ext_image_capture_source_v1::ExtImageCaptureSourceV1);
wayland_client::delegate_noop!(State: ignore ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1);
wayland_client::delegate_noop!(State: ignore ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1);

fn transform_frame(frame: Frame, transform: Option<wl_output::Transform>) -> crate::Result<Frame> {
    use image::imageops;
    use wl_output::Transform;

    let transform = transform.unwrap_or(Transform::Normal);
    if transform == Transform::Normal {
        return Ok(frame);
    }

    let image = image::RgbaImage::from_raw(frame.width, frame.height, frame.data)
        .ok_or(crate::Error::InvalidImageBuffer)?;
    // Undo the transform already applied by the compositor to the buffer.
    let image = match transform {
        Transform::_90 => imageops::rotate90(&image),
        Transform::_180 => imageops::rotate180(&image),
        Transform::_270 => imageops::rotate270(&image),
        Transform::Flipped => imageops::flip_horizontal(&image),
        Transform::Flipped90 => imageops::rotate270(&imageops::flip_horizontal(&image)),
        Transform::Flipped180 => imageops::rotate180(&imageops::flip_horizontal(&image)),
        Transform::Flipped270 => imageops::rotate90(&imageops::flip_horizontal(&image)),
        _ => return Err(crate::Error::WaylandScreencopyFailed),
    };

    Ok(Frame {
        width: image.width(),
        height: image.height(),
        data: image.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_interleaved_capture_events_to_their_requests() {
        use std::os::unix::net::UnixStream;
        use wayland_client::Proxy;

        let (socket, _server) = UnixStream::pair().unwrap();
        let connection = Connection::from_socket(socket).unwrap();
        let queue = connection.new_event_queue::<State>();
        let qh = queue.handle();
        let mut state = State::default();
        state.captures.insert(0, CaptureState::default());
        state.captures.insert(1, CaptureState::default());

        fn send<P: Proxy>(
            state: &mut State,
            connection: &Connection,
            qh: &QueueHandle<State>,
            id: usize,
            event: P::Event,
        ) where
            State: Dispatch<P, usize>,
        {
            let proxy = P::inert(connection.backend().downgrade());
            <State as Dispatch<P, usize>>::event(state, &proxy, event, &id, connection, qh);
        }

        let mut wlr = |id, event| {
            send::<zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1>(
                &mut state,
                &connection,
                &qh,
                id,
                event,
            );
        };
        wlr(
            1,
            zwlr_screencopy_frame_v1::Event::Buffer {
                format: WEnum::Value(wl_shm::Format::Xrgb8888),
                width: 2,
                height: 3,
                stride: 8,
            },
        );
        wlr(0, zwlr_screencopy_frame_v1::Event::Failed);
        wlr(
            1,
            zwlr_screencopy_frame_v1::Event::Ready {
                tv_sec_hi: 0,
                tv_sec_lo: 0,
                tv_nsec: 0,
            },
        );

        assert!(state.captures[&0].failed);
        assert!(!state.captures[&0].ready);
        assert!(state.captures[&1].ready);
        assert!(state.captures[&1].check_error().is_ok());
        assert_eq!(state.captures[&1].buffer_info.as_ref().unwrap().width, 2);

        state.captures.clear();
        state.captures.insert(2, CaptureState::default());
        state.captures.insert(3, CaptureState::default());

        let mut session = |id, event| {
            send::<ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1>(
                &mut state,
                &connection,
                &qh,
                id,
                event,
            );
        };
        use ext_image_copy_capture_session_v1::Event;
        session(
            2,
            Event::BufferSize {
                width: 4,
                height: 5,
            },
        );
        session(
            3,
            Event::BufferSize {
                width: 6,
                height: 7,
            },
        );
        session(
            3,
            Event::ShmFormat {
                format: WEnum::Value(wl_shm::Format::Argb8888),
            },
        );
        session(
            2,
            Event::ShmFormat {
                format: WEnum::Value(wl_shm::Format::Xrgb8888),
            },
        );
        session(3, Event::Done);
        session(2, Event::Done);
        // Late events from the preceding batch must not affect new requests.
        session(0, Event::Stopped);

        for (id, width, height, format) in [
            (2, 4, 5, wl_shm::Format::Xrgb8888),
            (3, 6, 7, wl_shm::Format::Argb8888),
        ] {
            let capture = &state.captures[&id];
            let info = capture.buffer_info.as_ref().unwrap();
            assert!(capture.constraints_done);
            assert!(capture.check_error().is_ok());
            assert_eq!(
                (info.width, info.height, info.format),
                (width, height, format)
            );
        }
    }

    #[test]
    fn restores_all_buffer_transforms() {
        use image::imageops;
        use wl_output::Transform;

        let original =
            image::RgbaImage::from_fn(3, 2, |x, y| image::Rgba([(y * 3 + x) as u8, 0, 0, 255]));

        let cases = [
            (Transform::Normal, original.clone()),
            (Transform::_90, imageops::rotate270(&original)),
            (Transform::_180, imageops::rotate180(&original)),
            (Transform::_270, imageops::rotate90(&original)),
            (Transform::Flipped, imageops::flip_horizontal(&original)),
            (
                Transform::Flipped90,
                imageops::rotate270(&imageops::flip_horizontal(&original)),
            ),
            (
                Transform::Flipped180,
                imageops::rotate180(&imageops::flip_horizontal(&original)),
            ),
            (
                Transform::Flipped270,
                imageops::rotate90(&imageops::flip_horizontal(&original)),
            ),
        ];

        for (transform, buffer) in cases {
            let frame = transform_frame(
                Frame {
                    width: buffer.width(),
                    height: buffer.height(),
                    data: buffer.into_raw(),
                },
                Some(transform),
            )
            .unwrap();

            assert_eq!(
                (frame.width, frame.height),
                original.dimensions(),
                "{transform:?}"
            );
            assert_eq!(frame.data, original.as_raw().as_slice(), "{transform:?}");
        }
    }
}
