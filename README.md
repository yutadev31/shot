# shot

Capture monitors as PNG images on Linux (X11 or Wayland with the wlr
screencopy protocol) and Windows 10 version 1703 or later.

Build with `cargo build --release --locked`.

```sh
# Capture all monitors, preserving their desktop positions.
shot > screenshot.png

# Capture one monitor by zero-based index.
shot --monitor 0 > screenshot.png
```

On Windows, monitor names are display device names such as `\\.\DISPLAY1`.
For example, in Command Prompt:

```bat
shot.exe --monitor \\.\DISPLAY1 > screenshot.png
```

Use Command Prompt or PowerShell 7.4 or later for PNG redirection. Older
PowerShell versions convert native program output to text and can corrupt PNGs.

The Rust `shot::capturer::Capturer` API also supports listing monitor names and
capturing a region. Coordinates and image sizes on Windows use physical pixels,
including when monitors have different DPI scaling. Capture requires an
interactive desktop; protected content may be blank.
