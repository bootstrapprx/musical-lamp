# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

- Build: `cargo build`
- Test: `cargo test`
- Run CLI/TUI: `cargo run -- [args]`
- Check: `cargo check`

## Architecture

`kodoo-ops` is a functional core (no UI) with CLI and interactive TUI frontends for managing deployments and `.env` lifecycles of Kodoo products (`trivia`, `accounting`).

- **Core API (`src/lib.rs` / `Ops`)**: Synchronous operations (`env_*`, `validate`, `generate`) and asynchronous job runner (`run`, `release`, `poll`).
- **EnvFile (`src/envfile.rs`)**: Robust `.env` parser and writer preserving order, comments, and blank lines, with 0600 permissions and atomic saves with backup rotation (`.bak-<unix>`).
- **Runner (`src/runner.rs`)**: Spawns asynchronous worker threads for docker/bench/site tasks, emitting events (`Event::Started`, `Event::StepStarted`, `Event::Line`, `Event::Finished`) polled by UI/CLI per frame.
- **Validation (`src/validate.rs`)**: Checks `.env` configuration, drift against `.env.example`, and build prerequisites.
- **TUI (`src/main.rs`, `slt/`)**: SuperLightTUI interactive interface supporting dashboard, dev-ops, system monitor, data explorer, logs, and wizard views.
- **Products (`src/product.rs`)**: Manages product definitions (`Product::Trivia`, `Product::Accounting`) and path resolution (`Paths`).
