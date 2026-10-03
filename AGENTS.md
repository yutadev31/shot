# AGENTS.md

## Project overview

`shot` is a Rust command-line screenshot tool. It captures one monitor or all
connected monitors, encodes the result as PNG, saves it under the user's
Pictures directory, and copies the image to the system clipboard.

The supported capture backends are:

- Wayland via `wlr-screencopy`.
- X11 via RandR monitor discovery and `XGetImage`.
- Windows via Win32/GDI.

On Linux, backend selection is automatic: Wayland is selected when
`WAYLAND_DISPLAY` is set; otherwise X11 is selected when `DISPLAY` is set.
The CLI can select a monitor by zero-based index or backend-reported name. With
multiple monitors and no explicit selection, it offers an interactive terminal
selector or, with `--rofi`, delegates selection to `rofi`.

## Repository layout

- `src/main.rs` — CLI parsing, backend selection flow, and application entry point.
- `src/capturer.rs` — backend creation, PNG encoding, file output, and clipboard handling.
- `src/backend/mod.rs` — backend trait, target types, frame stitching, and shared tests.
- `src/backend/wayland.rs` — Wayland output discovery and screencopy implementation.
- `src/backend/x11.rs` — X11 monitor discovery and pixel conversion.
- `src/backend/windows.rs` — Win32 monitor enumeration and GDI capture.
- `src/selector/cli.rs` — terminal monitor selector.
- `src/selector/rofi.rs` — `rofi`-based monitor selector.
- `src/frame.rs` — captured RGBA frame type.
- `src/error.rs` — typed application errors.
- `Cargo.toml` / `Cargo.lock` — package metadata, features, and locked dependencies.
- `flake.nix` — reproducible development shell and Wayland build dependencies.

## Development workflow

Use the Nix development shell when available:

```sh
nix develop
```

Run the standard checks from the repository root:

```sh
cargo fmt --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Build and inspect the CLI with:

```sh
cargo build
cargo run -- --help
cargo run -- --monitor 0
```

Full capture validation requires a live graphical session. Wayland needs
`WAYLAND_DISPLAY`, a compositor providing `wlr-screencopy`, and an
`arboard`-compatible `wayland-data-control` clipboard environment. X11 needs
`DISPLAY`. The `--rofi` selector additionally requires the `rofi` executable.

There are unit tests for shared frame stitching, but the display backends and
clipboard path are primarily validated through live capture.

## Implementation guidance

- Preserve the `Backend` abstraction. Keep display-system-specific discovery,
  capture, and pixel conversion in the corresponding backend module.
- Backends return `Frame` values containing tightly packed RGBA bytes. Keep PNG
  encoding, timestamped file creation, and clipboard handling in
  `capturer.rs`.
- Monitor indices are zero-based. Validate them during backend initialization or
  selection and return `MonitorOutOfRange`; never silently choose another monitor.
- `--all` must stitch frames using their display coordinates so negative
  positions and gaps in a multi-monitor layout remain meaningful.
- Prefer typed `Error` variants and `?` propagation over ad-hoc panics or
  `unwrap`. Any `unsafe` code should remain narrowly scoped and justified,
  especially in the Win32 backend.
- Keep successful output stable: save a timestamped PNG in
  `Pictures/Screenshots` and report the path and clipboard result on stdout.
- When changing dependencies or feature flags, update `Cargo.toml` and
  `Cargo.lock` as appropriate, and keep the Nix development environment in
  sync with build-time requirements.

## Style and review expectations

- Follow rustfmt defaults and the existing Rust 2024 style.
- Keep public API changes intentional; `Capturer` is the library's public entry
  point.
- Consider Wayland, X11, and Windows behavior separately when changing monitor
  enumeration, coordinates, pixel formats, or error handling.
- Preserve CLI compatibility, including monitor names, `--all`, `--rofi`, and
  the hidden clipboard-daemon path used internally on Linux.
- Avoid committing generated build output such as `target/`.
- Before submitting changes, run formatting and compile checks. Perform a live
  capture when the change affects a display backend, clipboard, or PNG output.
- After completing work, output an English commit message following the
  Conventional Commits specification.
