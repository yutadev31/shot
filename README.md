# shot

A lightweight, cross-platform screenshot utility written in Rust.

`shot` captures screenshots and writes PNG, JPEG, or WebP images to standard output, making it easy to save screenshots to files or pipe them into other commands.

## Features

- Capture a single monitor or all monitors.
- Select a monitor by index or name.
- Combine multiple monitors into one image while preserving their desktop positions.
- Output PNG, JPEG, or lossless WebP images directly to standard output.
- Support X11, Wayland, Windows, and macOS.

## Installation

### Cargo

```sh
cargo install --git https://github.com/yutadev31/shot
```

### Nix

```sh
nix profile install github:yutadev31/shot
```

## Usage

### Capture all monitors

```sh
shot > screenshot.png
```

When multiple monitors are connected, their screenshots are combined into a single image.

### Select the output format

PNG is the default. Use `--format` to select JPEG or lossless WebP.

```sh
shot --format jpeg > screenshot.jpg
shot --format webp > screenshot.webp
```

`--format png` explicitly selects PNG. All formats support monitor and region captures.

### Capture a specific monitor

Select a monitor by its zero-based index:

```sh
shot --monitor 0 > screenshot.png
```

Or select it by name:

```sh
shot --monitor HDMI-A-1 > screenshot.png
```

The short option `-m` is also available.

### Capture a region

Use desktop coordinates in `X,Y WIDTHxHEIGHT` format (negative coordinates are supported):

```sh
shot --region "100,200 640x480" > screenshot.png
shot -r $(slurp) > screenshot.png
```

Quoted slurp output (`shot -r "$(slurp)"`) is also supported. The region must
fit within the desktop image. `--region` cannot be combined with `--monitor`.

On Wayland, region coordinates use the compositor's logical desktop geometry
and require the `xdg-output` protocol. Each output is cropped at its own buffer
scale; images spanning different scales use the largest selected scale.

### Copy a screenshot to the clipboard

Use [cpst](https://github.com/yutadev31/cpst) to copy a screenshot directly to the system clipboard:

```sh
shot | cpy
```

### Save and copy simultaneously

```sh
shot | tee screenshot.png | cpy
```

**Note:** `shot` does not write image data directly to a terminal. Redirect or pipe its standard output.

## Platform Support

| Platform | Backend | Status |
|---|---|---|
| Linux | Wayland | Supported |
| Linux | X11 | Supported |
| Windows | Win32 | Supported |
| macOS | ScreenCaptureKit | Supported |

On Wayland, the compositor must support at least one of the following screenshot protocols:

- `ext-image-copy-capture-v1`
- `wlr-screencopy-unstable-v1`

On Windows, version 1703 or later of Windows 10 is required. Use Command Prompt or PowerShell 7.4 or later when redirecting PNG output.

On macOS, capture requires macOS 14 or later and screen recording permission
in System Settings under Privacy & Security. Monitor names use the form
`Display-<id>`, with the primary display at index 0. Single-monitor captures
use the display's backing resolution. Combined screenshots and desktop regions
use logical desktop coordinates and one output pixel per point.

Screenshot capture requires an active graphical desktop session.

**Note**: macOS support is currently untested due to a lack of access to macOS hardware.

## Development

Enter the Nix development environment:

```sh
nix develop
```

Run checks with Cargo:

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

Build the release binary:

```sh
cargo build --release
```

## License

Licensed under the [MIT License](LICENSE.txt).
