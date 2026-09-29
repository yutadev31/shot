# AGENTS.md

## Project overview

`shot` is a Rust command-line tool that captures a selected monitor and writes
the resulting PNG both to the clipboard and to `~/Pictures/Screenshots`.
It supports Wayland through `wlr-screencopy` and X11 through RandR/XGetImage.

## Repository layout

- `src/main.rs` — CLI parsing and application entry point.
- `src/capturer.rs` — backend selection, PNG encoding, file output, and clipboard output.
- `src/backend/mod.rs` — backend trait and module declarations.
- `src/backend/wayland.rs` — Wayland output discovery and screencopy implementation.
- `src/backend/x11.rs` — X11 monitor discovery and pixel conversion.
- `src/frame.rs` — captured-frame data types.
- `src/error.rs` — application error types.
- `Cargo.toml` — Rust dependencies and package metadata.
- `flake.nix` — reproducible development shell with Rust, rust-analyzer, taplo, and Wayland.

## Development workflow

Use the Nix shell when available:

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

There are currently no repository tests. `cargo test` still verifies that the
library and binary compile in test mode. Full capture validation requires a
live graphical session: Wayland needs `WAYLAND_DISPLAY`, a compositor with
`wlr-screencopy` and `wayland-data-control`; X11 needs `DISPLAY`.

Build and try the CLI with:

```sh
cargo run -- --help
cargo run -- --monitor 0
```

## Implementation guidance

- Preserve the `Backend` abstraction when changing capture behavior; keep
  display-system-specific code in its corresponding backend module.
- Backends return `Frame` values containing RGBA bytes. Keep conversion to PNG,
  file storage, and clipboard handling in `capturer.rs`.
- Monitor indices are zero-based and are resolved during backend initialization.
  Return `MonitorOutOfRange` rather than silently falling back to another
  monitor.
- Prefer typed `Error` variants and `?` propagation over ad-hoc panics or
  `unwrap`. Any `unsafe` code should remain narrowly scoped and justified.
- Keep output behavior stable: successful runs save a timestamped PNG and
  report the file path and clipboard result on stdout.
- When changing dependencies or their features, update both `Cargo.toml` and
  `Cargo.lock` as appropriate. Keep the Nix development environment in sync
  with build-time requirements.

## Style and review expectations

- Follow `rustfmt` defaults and the existing Rust 2024 style.
- Keep public API changes intentional; `Capturer` is the library's public entry
  point.
- Consider Wayland and X11 behavior separately when modifying monitor
  enumeration, pixel formats, or error handling.
- Avoid committing generated build output such as `target/`.
- Before submitting changes, run the formatting and compile checks above, and
  perform a live capture when the change affects a display backend, clipboard,
  or PNG output.
- After completing work, output a commit message in English that follows the Conventional Commits specification.
