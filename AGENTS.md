# Development notes for cadenza-shell

- **No `view!` macros:** Use manual, imperative relm4 implementation instead of `view!` macro for
  cleaner, more maintainable code.
- **Async**: Use relm4 commands to spawn async tasks.
  - `sender.oneshot_command` for a simple async task
  - `sender.command` for long-running services that need shutdown hooks
