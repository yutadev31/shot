use std::{ptr, sync::mpsc, time::Duration};

use block2::RcBlock;
use objc2::{AnyThread, rc::autoreleasepool};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGDisplayBounds, CGError,
    CGGetActiveDisplayList, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo, CGMainDisplayID,
    CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess, kCGColorSpaceSRGB,
};
use objc2_foundation::{NSArray, NSError, NSProcessInfo};
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCScreenshotManager, SCShareableContent, SCStreamConfiguration,
};

use super::{Backend, BackendTarget, Frame, PositionedFrame, stitch_positioned};

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(15);

pub struct MacosBackend {
    monitors: Vec<Monitor>,
    selected_monitor: usize,
}

#[derive(Clone, Copy)]
struct Monitor {
    id: u32,
    x: i32,
    y: i32,
    width: usize,
    height: usize,
}

fn macos_error(operation: &'static str, message: impl Into<String>) -> crate::Error {
    crate::Error::Macos {
        operation,
        message: message.into(),
    }
}

impl Backend for MacosBackend {
    fn initialize(target: BackendTarget) -> crate::Result<Self> {
        let version = autoreleasepool(|_| NSProcessInfo::processInfo().operatingSystemVersion());

        if version.majorVersion < 14 {
            return Err(macos_error(
                "initialization",
                "macOS 14 or later is required",
            ));
        }

        let mut count = 0;
        // SAFETY: A null display buffer queries the number of active displays.
        let status = unsafe { CGGetActiveDisplayList(0, ptr::null_mut(), &mut count) };

        if status != CGError::Success {
            return Err(macos_error("CGGetActiveDisplayList", format!("{status:?}")));
        }

        let mut ids = vec![0; count as usize];
        if count != 0 {
            // SAFETY: ids has room for count display IDs and written is a valid out pointer.
            let mut written = 0;
            let status = unsafe { CGGetActiveDisplayList(count, ids.as_mut_ptr(), &mut written) };

            if status != CGError::Success {
                return Err(macos_error("CGGetActiveDisplayList", format!("{status:?}")));
            }

            ids.truncate(written as usize);
        }

        if ids.is_empty() {
            return Err(crate::Error::NoMonitors);
        }

        let main = CGMainDisplayID();
        ids.sort_unstable_by_key(|&id| (id != main, id));

        let monitors = ids
            .into_iter()
            .map(|id| {
                let bounds = CGDisplayBounds(id);
                let width = bounds.size.width as usize;
                let height = bounds.size.height as usize;

                if width == 0 || height == 0 {
                    return Err(macos_error(
                        "CGDisplayBounds",
                        format!("display {id} has no area"),
                    ));
                }

                Ok(Monitor {
                    id,
                    x: bounds.origin.x as i32,
                    y: bounds.origin.y as i32,
                    width,
                    height,
                })
            })
            .collect::<crate::Result<Vec<_>>>()?;

        let mut backend = Self {
            monitors,
            selected_monitor: 0,
        };

        if let BackendTarget::Monitor(index) = target {
            backend.select_monitor(index)?;
        }

        Ok(backend)
    }

    fn capture_output(&mut self) -> crate::Result<Frame> {
        let monitor = self.monitors[self.selected_monitor];
        let mut frames = capture_monitors(&[monitor], true)?;
        Ok(frames.remove(0).frame)
    }

    fn capture_all_outputs(&mut self) -> crate::Result<Frame> {
        stitch_positioned(&capture_monitors(&self.monitors, false)?)
    }

    fn desktop_origin(&self) -> (i32, i32) {
        self.monitors
            .iter()
            .fold((i32::MAX, i32::MAX), |(x, y), monitor| {
                (x.min(monitor.x), y.min(monitor.y))
            })
    }

    fn monitor_count(&self) -> usize {
        self.monitors.len()
    }

    fn monitor_names(&self) -> Vec<String> {
        self.monitors
            .iter()
            .map(|monitor| format!("Display-{}", monitor.id))
            .collect()
    }

    fn select_monitor(&mut self, index: usize) -> crate::Result<()> {
        if index >= self.monitors.len() {
            return Err(crate::Error::MonitorOutOfRange {
                index,
                count: self.monitors.len(),
            });
        }

        self.selected_monitor = index;
        Ok(())
    }
}

fn capture_monitors(
    monitors: &[Monitor],
    native_resolution: bool,
) -> crate::Result<Vec<PositionedFrame>> {
    if !CGPreflightScreenCaptureAccess() && !CGRequestScreenCaptureAccess() {
        return Err(macos_error(
            "screen capture",
            "grant screen recording access in System Settings > Privacy & Security",
        ));
    }

    let (sender, receiver) = mpsc::channel();
    let count = monitors.len();
    let monitors = monitors.to_vec();

    let completion = RcBlock::new(
        move |content: *mut SCShareableContent, error: *mut NSError| {
            // SAFETY: ScreenCaptureKit keeps callback arguments alive throughout this call.
            let displays = unsafe { content.as_ref() }
                .ok_or_else(|| callback_error("enumerate capturable displays", error))
                // SAFETY: The initialized content object owns the returned display list.
                .map(|content| unsafe { content.displays().to_vec() });

            let displays = match displays {
                Ok(displays) => displays,
                Err(error) => {
                    // A timed-out caller may have dropped the receiver.
                    let _ = sender.send(Err(error));
                    return;
                }
            };

            for (index, &monitor) in monitors.iter().enumerate() {
                // SAFETY: Each display is retained by the local list.
                let display = displays
                    .iter()
                    .find(|display| unsafe { display.displayID() } == monitor.id);

                if let Some(display) = display {
                    let sender = sender.clone();
                    capture_display(display, monitor, native_resolution, move |result| {
                        // A timed-out caller may have dropped the receiver.
                        let _ = sender.send(result.map(|frame| (index, frame)));
                    });
                } else {
                    let _ = sender.send(Err(macos_error(
                        "select display",
                        format!("display {} is no longer available", monitor.id),
                    )));
                    return;
                }
            }
        },
    );

    // SAFETY: ScreenCaptureKit copies the completion block for asynchronous use.
    unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&completion) };
    drop(completion);

    let mut frames = (0..count)
        .map(|_| {
            receiver
                .recv_timeout(CAPTURE_TIMEOUT)
                .map_err(|error| macos_error("screen capture", error.to_string()))?
        })
        .collect::<crate::Result<Vec<_>>>()?;

    frames.sort_unstable_by_key(|(index, _)| *index);
    Ok(frames.into_iter().map(|(_, frame)| frame).collect())
}

fn capture_display(
    display: &SCDisplay,
    monitor: Monitor,
    native_resolution: bool,
    send: impl Fn(crate::Result<PositionedFrame>) + Send + Sync + 'static,
) {
    // SAFETY: The display is live, the window list is empty, and each request owns its configuration.
    unsafe {
        let filter = SCContentFilter::initWithDisplay_excludingWindows(
            SCContentFilter::alloc(),
            display,
            &NSArray::new(),
        );
        let config = SCStreamConfiguration::new();
        let scale = if native_resolution {
            f64::from(filter.pointPixelScale())
        } else {
            1.0
        };
        config.setWidth((monitor.width as f64 * scale).round() as usize);
        config.setHeight((monitor.height as f64 * scale).round() as usize);
        config.setShowsCursor(false);

        let completion = RcBlock::new(move |image: *mut CGImage, error: *mut NSError| {
            let result = image
                .as_ref()
                .ok_or_else(|| callback_error("capture display", error))
                .and_then(image_to_frame)
                .map(|frame| PositionedFrame {
                    frame,
                    x: monitor.x,
                    y: monitor.y,
                });

            send(result);
        });

        SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
            &filter,
            &config,
            Some(&completion),
        );
    }
}

fn callback_error(operation: &'static str, error: *mut NSError) -> crate::Error {
    // SAFETY: This helper is called while the originating callback's NSError is live.
    let message = unsafe { error.as_ref() }.map_or_else(
        || "ScreenCaptureKit returned no result".to_string(),
        |error| error.localizedDescription().to_string(),
    );

    macos_error(operation, message)
}

fn image_to_frame(image: &CGImage) -> crate::Result<Frame> {
    let width =
        u32::try_from(CGImage::width(Some(image))).map_err(|_| crate::Error::InvalidImageBuffer)?;
    let height = u32::try_from(CGImage::height(Some(image)))
        .map_err(|_| crate::Error::InvalidImageBuffer)?;
    let stride = (width as usize)
        .checked_mul(4)
        .ok_or(crate::Error::InvalidImageBuffer)?;
    let len = stride
        .checked_mul(height as usize)
        .filter(|&len| len != 0 && len <= isize::MAX as usize)
        .ok_or(crate::Error::InvalidImageBuffer)?;
    let mut data = vec![0; len];

    // SAFETY: kCGColorSpaceSRGB is a process-lifetime Core Graphics constant.
    let color_space =
        CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB })).ok_or_else(|| {
            macos_error(
                "CGColorSpaceCreateWithName",
                "could not create an sRGB color space",
            )
        })?;
    let bitmap_info = CGImageByteOrderInfo::Order32Big.0 | CGImageAlphaInfo::NoneSkipLast.0;

    // SAFETY: data is writable for stride * height bytes and outlives the context.
    let context = unsafe {
        CGBitmapContextCreate(
            data.as_mut_ptr().cast(),
            width as usize,
            height as usize,
            8,
            stride,
            Some(&color_space),
            bitmap_info,
        )
    }
    .ok_or_else(|| macos_error("CGBitmapContextCreate", "could not create an sRGB bitmap"))?;

    CGContext::draw_image(
        Some(&context),
        CGRect::new(
            CGPoint::ZERO,
            CGSize::new(f64::from(width), f64::from(height)),
        ),
        Some(image),
    );
    drop(context);

    for pixel in data.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }

    Ok(Frame {
        data,
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use objc2_core_graphics::CGBitmapContextCreateImage;

    use super::*;

    #[test]
    fn converts_bgra_to_top_down_opaque_rgba() {
        let mut source = [0_u8, 0, 255, 255, 255, 0, 0, 255];

        // SAFETY: The sRGB constant is valid and source outlives the context and image.
        let image = unsafe {
            let color_space = CGColorSpace::with_name(Some(kCGColorSpaceSRGB)).unwrap();
            let info =
                CGImageByteOrderInfo::Order32Little.0 | CGImageAlphaInfo::PremultipliedFirst.0;
            let context = CGBitmapContextCreate(
                source.as_mut_ptr().cast(),
                1,
                2,
                8,
                4,
                Some(&color_space),
                info,
            )
            .unwrap();
            CGBitmapContextCreateImage(Some(&context)).unwrap()
        };

        let frame = image_to_frame(&image).unwrap();
        assert_eq!((frame.width, frame.height), (1, 2));
        assert_eq!(frame.data, [255, 0, 0, 255, 0, 0, 255, 255]);
    }
}
