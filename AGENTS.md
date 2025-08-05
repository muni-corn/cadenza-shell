# AGENTS.md - Development Guide for muse-shell

## Build/Test Commands

- **Build**: `nix build` (builds the shell using flake.nix)
- **Dev shell**: `nix develop` (enters development environment with AGS)
- **Lint**: `biome check .` (lints with Biome)
- **Format**: `biome format --write .` (formats code)
- **Type check**: `tsc` (TypeScript type checking)
- DO NOT use `ags run` to test the shell. This command blocks indefinitely and
  is unsuitable for agentic coding.
- **IMPORTANT**: New files must be added to git index (`git add`) before `nix build`
  will pick them up, as Nix only includes tracked files in the build.

## Code Style & Conventions

- **Language**: TypeScript with JSX (React-style components using AGS/GTK4)
- **Formatting**: Biome formatter with double quotes, space indentation
- **Imports**: Use relative imports for local modules, absolute for external
  packages
- **Naming**: camelCase for variables/functions, PascalCase for components
- **File structure**: Components in `widget/`, utilities in `widget/utils.tsx`
- **Types**: Strict TypeScript enabled, define interfaces in separate `.ts`
  files when complex
- **Error handling**: Use try/catch for async operations, graceful degradation
  for missing services
- **State**: Use AGS's `createState`, `createBinding`, `createComputed` for
  reactive state
- **Components**: Export named functions, use destructured props, follow
  existing patterns
- **Icons**: Use icon constants arrays, implement fallbacks for missing icons
- **Services**: Import from `gi://` namespace (e.g., `gi://AstalBattery`)

## Development Tips

- Use the context7 MCP server to look up documentation for astal, ags v3, and
  gnim.
