# AGENTS.md - Development Guide for cadenza-shell

## Build/Test Commands

### Rust Implementation (Current)

- **Build**: `cargo build` or `cargo build --release` (builds the Rust shell)
- **Run**: `cargo run` (runs the shell directly)
- **Check**: `cargo check` (fast compilation check without building)
- **Test**: `cargo test` (runs unit tests)
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

- **Language**: Rust with GTK4-rs bindings
- **Imports**: Use `use` statements, group by std/external/local
- **File structure**:
  - Services in `src/services/`
  - Widgets in `src/widgets/`
  - Status bar tiles in `src/tiles/`
  - Utilities in `src/utils/`
  - Styling in `src/style/`
- **Icons**: Use const arrays in `src/utils/icons.rs`
- **Components**: Prefer manual relm4 implementation over `view!` macro for
  cleaner, more maintainable code. Follow the pattern in `src/tiles/clock.rs`
  and `src/tiles/weather.rs`
- **Comments**: Stylize all line comments in all lowercase. Doc comments should
  use sentence case.

## Development Tips

- Use the context7 MCP server to look up documentation for any library
- For GObject subclassing, use the `glib::Properties` derive macro
- Always make small, atomic, incremental, granular git commits while you work;
  do not add co-author footers, and follow conventional commit spec (see
  `git log` for examples)
