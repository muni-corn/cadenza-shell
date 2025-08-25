# AGENTS.md - Development Guide for cadenza-shell

## Build/Test Commands

### Rust Implementation (Current)

- **Build**: `cargo build` or `cargo build --release` (builds the Rust shell)
- **Run**: `cargo run` (runs the shell directly)
- **Check**: `cargo check` (fast compilation check without building)
- **Test**: `cargo test` (runs unit tests)
- **Lint**: `cargo clippy` (Rust linter)
- **Format**: `cargo fmt` (Rust formatter)
- **Dev shell**: `nix develop` (enters development environment with Rust
  toolchain)
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
- **Formatting**: `cargo fmt` with default rustfmt settings
- **Imports**: Use `use` statements, group by std/external/local
- **Naming**: snake_case for variables/functions, PascalCase for types/structs
- **File structure**:
  - Services in `src/services/`
  - Widgets in `src/widgets/` and `src/tiles/`
  - Utilities in `src/utils/`
  - Styling in `src/style/`
- **Error handling**: Use `Result<T, E>` and `anyhow::Error`, graceful
  degradation
- **State**: Use GTK4's native property bindings and GObject signals
- **Async**: Use `tokio` runtime with `glib::spawn_future_local` for GTK
  integration
- **Memory**: Use `Rc<RefCell<>>` for shared mutable state, `Arc<Mutex<>>` for
  thread-safe sharing
- **Icons**: Use const arrays in `src/utils/icons.rs`
- **Services**: Implement as GObject subclasses with properties
- **Components**: Prefer manual relm4 implementation over `view!` macro for cleaner, more maintainable code. Follow the pattern in `src/tiles/clock.rs` and `src/tiles/weather.rs`

### Legacy TypeScript Implementation

- **Language**: TypeScript with JSX (React-style components using AGS/GTK4)
- **Formatting**: Biome formatter with double quotes, space indentation
- **Imports**: Use relative imports for local modules, absolute for external
  packages
- **Naming**: camelCase for variables/functions, PascalCase for components
- **File structure**: Components in `src/`, utilities in `src/utils.tsx`
- **Error handling**: Use try/catch for async operations, graceful degradation
- **State**: Use AGS's `createState`, `createBinding`, `createComputed`
- **Services**: Import from `gi://` namespace (e.g., `gi://AstalBattery`)

## Development Tips

- Use the context7 MCP server to look up documentation for gtk4-rs, GTK4,
  glib-rs, and other Rust GTK libraries
- Consult https://gtk-rs.org/gtk4-rs/stable/latest/book/ for GTK4-rs guidance
- Use `GTK_DEBUG=interactive cargo run` to enable GTK Inspector for debugging
- For GObject subclassing, use the `glib::Properties` derive macro
- Use `glib::clone!` macro for closure captures to avoid reference issues
- Always make small, atomic, incremental, granular git commits while you work.
  Do not add co-author footers.
