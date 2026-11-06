# Repository Guidelines

## Development Workflow

Run commands from the repository root. Use the Nix development shell when available:

```sh
nix develop
```

Format and validate the workspace before submitting changes:

```sh
cargo fmt --all --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
```

## Development Principles

- Follow the style, structure, naming conventions, and patterns used by existing similar code in the repository whenever practical.
- Use blank lines to separate logical sections and improve readability.
- As a general rule, insert a blank line after multiline function calls and blocks such as `if` and `for`.
- Do not make temporary workarounds that prioritize immediate behavior over a fundamental solution, or introduce fixes that are not root-cause fixes.
- Do not silently ignore errors; handle them in a way that makes their causes identifiable whenever possible.
- Prioritize maintainability and runtime performance over ease of implementation.
- If the prompt is ambiguous or lacks necessary information, ask clarifying questions before proceeding.

## Git

- Do not run Git commands that modify the repository state unless explicitly instructed to do so.
- At the end of the task, provide an English commit message in the chat following the Conventional Commits specification.
