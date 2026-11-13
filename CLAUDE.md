# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands
- **Build**: `cargo build`
- **Check**: `cargo check`
- **Test**: `cargo test`
- **Run**: `cargo run`

## Architecture & Structure
- **musical-lamp-tui**: A Rust-based TUI application built with `ratatui`, `crossterm`, and `tokio` for orchestrating Multi-host Docker Compose + FinOps tasks.
- **`src/`**: Core TUI application source code:
  - `main.rs`: Entry point, sets up crossterm raw mode, alternate screen, async tick and input loops, and handles tunnel token prompts.
  - `app.rs`: Application state and event handling.
  - `domain/`: Domain models (`Node`, `Container`, `Product`, `StackState`, `OpsStatus`, cost allocation logic).
  - `ui/`: TUI layout and widgets.
  - `action.rs`: Action and event enums.
  - `telemetry/`: Telemetry providers for nodes and metrics.
- **`products/`**: Product configurations (`accounting`, `trivia`) containing `apps.json` and `build.env`.
- **`common/`**: Shared docker compose base files, versions, and build scripts.
