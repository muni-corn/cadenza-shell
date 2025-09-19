# AGENTS.md - Development Guide for cadenza-shell

## Build/Test Commands

### Rust Implementation (Current)

- **Build**: `cargo build` or `cargo build --release` (builds the Rust shell)
- **Run**: `cargo run` (runs the shell directly)
- **Check**: `cargo check` (fast compilation check without building)
- **Test**: `cargo test` (runs all unit tests)
- **Lint**: `cargo clippy` (Rust linter)
- **Format**: `nix fmt` (treefmt with rustfmt, biome, taplo, etc)
- **Nix build**: `nix build` (builds using Nix flake)
- **IMPORTANT**: New files must be added to git index (`git add`) before
  `nix build` will pick them up, as Nix only includes tracked files in the
  build.

### Legacy TypeScript Implementation

- **Build**: `nix build .#typescript` (builds the TypeScript shell)
- **Lint**: `biome check .` (lints with Biome)
- **Format**: `biome format --write .` (formats code)
- **Type check**: `tsc` (TypeScript type checking)
- Use `timeout` with `ags run src/app.ts` to test the shell.

## Code Style & Conventions

### Rust Implementation

- **Language**: Rust with GTK4-rs bindings and relm4 framework
- **Imports**: Use `use` statements, group by std/external/local with blank
  lines between groups
- **Error Handling**: Use `anyhow::Result` for fallible functions, `thiserror`
  for custom error types
- **Naming**: snake_case for functions/variables, PascalCase for types/structs,
  SCREAMING_SNAKE_CASE for constants
- **File Structure**:
  - Services in `src/services/` (background workers for system monitoring)
  - Widgets in `src/widgets/`
  - Status bar tiles in `src/tiles/`
  - Utilities in `src/utils/`
  - Tests in `src/tests/`
- **Services Pattern**: All services implement `relm4::Worker` trait with
  `Init`, `Input`, and `Output` types. Use enum variants for output messages
  (e.g., `BatteryUpdate`, `BluetoothWorkerOutput`). Services handle system
  monitoring via D-Bus (`zbus`), file watching (`inotify`), or direct system
  APIs
- **Icons**: Use const arrays in `src/utils/icons.rs`, add new icon names in
  `build.rs`
- **Components**: Prefer manual relm4 implementation over `view!` macro for
  cleaner, more maintainable code. Follow the pattern in `src/tiles/clock.rs`
- **Comments**: All line comments in lowercase. Doc comments use sentence case
  with proper punctuation.
- **Async**: Use `relm4::spawn` to spawn worker threads, use channels for
  communication

### TypeScript Implementation

- **Formatting**: Biome with 2-space indentation, double quotes for strings and
  JSX
- **Types**: Explicit typing preferred, avoid `any`
- **Imports**: Group and sort imports, prefer named imports

## Development Tips

- Use the context7 MCP server to look up documentation for any library
- Always make small, atomic, incremental, granular git commits while you work;
  do not add co-author footers, and follow conventional commit spec (see
  `git log` for examples)
