# Development notes for cadenza-shell

- **Async**: Use relm4 commands to spawn async tasks.
  - `sender.oneshot_command` for a simple async task
  - `sender.command` for long-running services that need shutdown hooks
