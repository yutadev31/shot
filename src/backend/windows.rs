use std::{mem::size_of, ptr};

use windows_sys::Win32::{
    Foundation::{LPARAM, RECT},
    Graphics::Gdi::*,
    UI::HiDpi::{
        DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        SetThreadDpiAwarenessContext,
    },
};

use crate::backend::{Backend, BackendTarget, Frame, PositionedFrame, stitch_positioned};

pub struct WindowsBackend {
    monitors: Vec<Monitor>,
    selected_monitor: Option<usize>,
}

struct Monitor {
    name: String,
    bounds: RECT,
}

fn windows_error(operation: &'static str) -> crate::Error {
    crate::Error::Windows {
        operation,
        source: std::io::Error::last_os_error(),
    }
}

// Thread-local DPI changes are restored before returning to the caller.
struct DpiContext(DPI_AWARENESS_CONTEXT);

impl DpiContext {
    fn enter() -> crate::Result<Self> {
        // SAFETY: The constant is a valid DPI context for the current thread.
        let previous =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

        if previous.is_null() {
            return Err(windows_error("SetThreadDpiAwarenessContext"));
        }

        Ok(Self(previous))
    }
}

impl Drop for DpiContext {
    fn drop(&mut self) {
        // SAFETY: This context was returned by a successful call on this thread.
        unsafe { SetThreadDpiAwarenessContext(self.0) };
    }
}

impl Backend for WindowsBackend {
    fn initialize(target: BackendTarget) -> crate::Result<Self> {
        let _dpi = DpiContext::enter()?;
        let mut monitors = Vec::<HMONITOR>::new();

        // SAFETY: The callback receives a live Vec pointer for this synchronous call.
        if unsafe {
            EnumDisplayMonitors(
                ptr::null_mut(),
                ptr::null(),
                Some(collect_monitor),
                &mut monitors as *mut Vec<HMONITOR> as LPARAM,
            )
        } == 0
        {
            return Err(windows_error("EnumDisplayMonitors"));
        }

        let monitors = monitors
            .into_iter()
            .map(|handle| {
                let mut info = MONITORINFOEXW::default();
                info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;

                // SAFETY: info has the required size and remains writable during the call.
                if unsafe { GetMonitorInfoW(handle, &mut info.monitorInfo) } == 0 {
                    return Err(windows_error("GetMonitorInfoW"));
                }

                let end = info
                    .szDevice
                    .iter()
                    .position(|&unit| unit == 0)
                    .unwrap_or(info.szDevice.len());
                Ok(Monitor {
                    name: String::from_utf16_lossy(&info.szDevice[..end]),
                    bounds: info.monitorInfo.rcMonitor,
                })
            })
            .collect::<crate::Result<Vec<_>>>()?;

        if monitors.is_empty() {
            return Err(crate::Error::NoMonitors);
        }

        let mut backend = Self {
            monitors,
            selected_monitor: None,
        };

        if let BackendTarget::Monitor(index) = target {
            backend.select_monitor(index)?;
        }

        Ok(backend)
    }

    fn capture_output(&mut self) -> crate::Result<Frame> {
        capture_monitor(&self.monitors[self.selected_monitor.unwrap_or(0)])
    }

    fn capture_all_outputs(&mut self) -> crate::Result<Frame> {
        let frames = self
            .monitors
            .iter()
            .map(|monitor| {
                Ok(PositionedFrame {
                    frame: capture_monitor(monitor)?,
                    x: monitor.bounds.left,
                    y: monitor.bounds.top,
                })
            })
            .collect::<crate::Result<Vec<_>>>()?;

        stitch_positioned(&frames)
    }

    /// Returns the origin of the stitched Windows monitor rectangles.
    fn desktop_origin(&self) -> (i32, i32) {
        self.monitors
            .iter()
            .fold((i32::MAX, i32::MAX), |origin, monitor| {
                (
                    origin.0.min(monitor.bounds.left),
                    origin.1.min(monitor.bounds.top),
                )
            })
    }

    fn monitor_count(&self) -> usize {
        self.monitors.len()
    }

    fn monitor_names(&self) -> Vec<String> {
        self.monitors
            .iter()
            .map(|monitor| monitor.name.clone())
            .collect()
    }

    fn select_monitor(&mut self, index: usize) -> crate::Result<()> {
        if index >= self.monitors.len() {
            return Err(crate::Error::MonitorOutOfRange {
                index,
                count: self.monitors.len(),
            });
        }

        self.selected_monitor = Some(index);
        Ok(())
    }
}

unsafe extern "system" fn collect_monitor(
    monitor: HMONITOR,
    _dc: HDC,
    _bounds: *mut RECT,
    data: LPARAM,
) -> i32 {
    // SAFETY: initialize passes a live, exclusively borrowed Vec for this callback.
    unsafe { &mut *(data as *mut Vec<HMONITOR>) }.push(monitor);
    1
}

struct CaptureResources {
    screen: HDC,
    memory: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
}

impl Drop for CaptureResources {
    fn drop(&mut self) {
        // SAFETY: Handles are owned by this guard. Deselect before deleting the bitmap.
        unsafe {
            if !self.previous.is_null() {
                SelectObject(self.memory, self.previous);
            }

            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }

            if !self.memory.is_null() {
                DeleteDC(self.memory);
            }

            if !self.screen.is_null() {
                ReleaseDC(ptr::null_mut(), self.screen);
            }
        }
    }
}

fn capture_monitor(monitor: &Monitor) -> crate::Result<Frame> {
    let _dpi = DpiContext::enter()?;
    let width = monitor
        .bounds
        .right
        .checked_sub(monitor.bounds.left)
        .filter(|&size| size > 0)
        .ok_or(crate::Error::InvalidImageBuffer)?;
    let height = monitor
        .bounds
        .bottom
        .checked_sub(monitor.bounds.top)
        .filter(|&size| size > 0)
        .ok_or(crate::Error::InvalidImageBuffer)?;
    let size = (width as usize)
        .checked_mul(height as usize)
        .and_then(|size| size.checked_mul(4))
        .ok_or(crate::Error::InvalidImageBuffer)?;
    let mut data = vec![0; size];

    // SAFETY: All pointers reference live buffers; each acquired GDI resource is guarded.
    unsafe {
        let mut resources = CaptureResources {
            screen: GetDC(ptr::null_mut()),
            memory: ptr::null_mut(),
            bitmap: ptr::null_mut(),
            previous: ptr::null_mut(),
        };

        if resources.screen.is_null() {
            return Err(windows_error("GetDC"));
        }

        resources.memory = CreateCompatibleDC(resources.screen);

        if resources.memory.is_null() {
            return Err(windows_error("CreateCompatibleDC"));
        }

        resources.bitmap = CreateCompatibleBitmap(resources.screen, width, height);

        if resources.bitmap.is_null() {
            return Err(windows_error("CreateCompatibleBitmap"));
        }

        let previous = SelectObject(resources.memory, resources.bitmap);

        if previous.is_null() || previous == GDI_ERROR as HGDIOBJ {
            return Err(windows_error("SelectObject"));
        }

        resources.previous = previous;

        if BitBlt(
            resources.memory,
            0,
            0,
            width,
            height,
            resources.screen,
            monitor.bounds.left,
            monitor.bounds.top,
            SRCCOPY | CAPTUREBLT,
        ) == 0
        {
            return Err(windows_error("BitBlt"));
        }

        // GetDIBits requires the bitmap to be deselected from the device context.
        let selected = SelectObject(resources.memory, resources.previous);

        if selected.is_null() || selected == GDI_ERROR as HGDIOBJ {
            return Err(windows_error("SelectObject (restore)"));
        }

        resources.previous = ptr::null_mut();
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };

        if GetDIBits(
            resources.screen,
            resources.bitmap,
            0,
            height as u32,
            data.as_mut_ptr().cast(),
            &mut info,
            DIB_RGB_COLORS,
        ) != height
        {
            return Err(windows_error("GetDIBits"));
        }
    }

    for pixel in data.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
        pixel[3] = 255;
    }

    Ok(Frame {
        data,
        width: width as u32,
        height: height as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::DpiContext;
    use windows_sys::Win32::UI::HiDpi::{
        AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        GetThreadDpiAwarenessContext,
    };

    #[test]
    fn restores_callers_dpi_context() {
        // SAFETY: These APIs read the current thread's DPI context.
        let previous = unsafe { GetThreadDpiAwarenessContext() };

        {
            let _guard = DpiContext::enter().expect("set per-monitor DPI awareness");

            // SAFETY: Both arguments are valid DPI contexts.
            assert_ne!(
                unsafe {
                    AreDpiAwarenessContextsEqual(
                        GetThreadDpiAwarenessContext(),
                        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                    )
                },
                0
            );
        }

        // SAFETY: Both arguments are valid DPI contexts from this thread.
        assert_ne!(
            unsafe { AreDpiAwarenessContextsEqual(GetThreadDpiAwarenessContext(), previous) },
            0
        );
    }
}
