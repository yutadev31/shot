# shot

A lightweight, cross-platform screenshot utility written in Rust.

`shot` captures screenshots and writes PNG images to standard output, making it easy to save screenshots to files or pipe them into other commands.

## Features

- Capture a single monitor or all monitors.
- Select a monitor by index or name.
- Combine multiple monitors into one image while preserving their desktop positions.
- Output PNG images directly to standard output.
- Support X11, Wayland, and Windows.

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

### Copy a screenshot to the clipboard

Use [cpst](https://github.com/yutadev31/cpst) to copy a screenshot directly to the system clipboard:

```sh
shot | cpy
```

### Save and copy simultaneously

```sh
shot | tee screenshot.png | cpy
```

**Note:** `shot` does not write PNG data directly to a terminal. Redirect or pipe its standard output.

## Platform Support

| Platform | Backend | Status |
|---|---|---|
| Linux | Wayland | Supported |
| Linux | X11 | Supported |
| Windows | Win32 | Supported |
| macOS | — | Not supported |

On Wayland, the compositor must support at least one of the following screenshot protocols:

- `ext-image-copy-capture-v1`
- `wlr-screencopy-unstable-v1`

On Windows, version 1703 or later of Windows 10 is required. Use Command Prompt or PowerShell 7.4 or later when redirecting PNG output.

Screenshot capture requires an active graphical desktop session.

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
