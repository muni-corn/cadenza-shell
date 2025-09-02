/// Integration examples for the command pattern in muse-shell
/// This shows how commands can be integrated with Relm4 components
use super::{AppCommand, Command, GlobalCommandExecutor};

/// Example integration with a tile widget showing how to use commands
pub struct TileCommandIntegration {
    command_executor: GlobalCommandExecutor,
}

impl TileCommandIntegration {
    pub fn new() -> Self {
        Self {
            command_executor: GlobalCommandExecutor::new(),
        }
    }

    /// Example: Handle a notification tile click
    pub fn handle_notification_tile_click(&mut self) -> Result<(), String> {
        self.command_executor
            .execute(AppCommand::ToggleNotificationCenter)
    }

    /// Example: Handle volume adjustment
    pub fn handle_volume_change(&mut self, new_volume: f64) -> Result<(), String> {
        self.command_executor
            .execute(AppCommand::SetVolume(new_volume))
    }

    /// Example: Handle workspace switching
    pub fn handle_workspace_switch(&mut self, workspace: u32) -> Result<(), String> {
        self.command_executor
            .execute(AppCommand::SwitchWorkspace(workspace))
    }

    /// Example: Undo last action
    pub fn undo_last_action(&mut self) -> Result<String, String> {
        self.command_executor.undo()
    }

    /// Example: Check if undo is available (for UI state)
    pub fn can_undo(&self) -> bool {
        self.command_executor.can_undo()
    }
}

/// Example of how to integrate commands into a Relm4 component
/// This would be added to the bar or tile components
pub mod relm4_integration {
    use super::*;

    /// Messages that components can send to request command execution
    #[derive(Debug)]
    pub enum CommandMsg {
        Execute(AppCommand),
        Undo,
        Redo,
    }

    /// Output from command execution (for status updates, etc.)
    #[derive(Debug)]
    pub enum CommandOutput {
        CommandExecuted(String),
        UndoPerformed(String),
        RedoPerformed(String),
        CommandFailed(String),
    }

    /// Example showing how commands might be integrated into a component's
    /// update method
    pub fn handle_command_message(
        msg: CommandMsg,
        command_executor: &mut GlobalCommandExecutor,
    ) -> Option<CommandOutput> {
        match msg {
            CommandMsg::Execute(command) => {
                let description = command.description();
                match command_executor.execute(command) {
                    Ok(()) => Some(CommandOutput::CommandExecuted(description)),
                    Err(e) => Some(CommandOutput::CommandFailed(e)),
                }
            }
            CommandMsg::Undo => match command_executor.undo() {
                Ok(description) => Some(CommandOutput::UndoPerformed(description)),
                Err(e) => Some(CommandOutput::CommandFailed(e)),
            },
            CommandMsg::Redo => match command_executor.redo() {
                Ok(description) => Some(CommandOutput::RedoPerformed(description)),
                Err(e) => Some(CommandOutput::CommandFailed(e)),
            },
        }
    }

    /// Example component state that includes command functionality
    #[derive(Debug)]
    pub struct ComponentWithCommands {
        pub command_executor: GlobalCommandExecutor,
        pub last_command_result: Option<String>,
    }

    impl ComponentWithCommands {
        pub fn new() -> Self {
            Self {
                command_executor: GlobalCommandExecutor::new(),
                last_command_result: None,
            }
        }

        pub fn process_command(&mut self, msg: CommandMsg) {
            if let Some(output) = handle_command_message(msg, &mut self.command_executor) {
                match output {
                    CommandOutput::CommandExecuted(desc)
                    | CommandOutput::UndoPerformed(desc)
                    | CommandOutput::RedoPerformed(desc) => {
                        self.last_command_result = Some(desc);
                        log::info!(
                            "Command result: {}",
                            self.last_command_result.as_ref().unwrap()
                        );
                    }
                    CommandOutput::CommandFailed(error) => {
                        log::error!("Command failed: {}", error);
                        self.last_command_result = Some(format!("Error: {}", error));
                    }
                }
            }
        }
    }

    impl Default for ComponentWithCommands {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tile_command_integration() {
        let mut integration = TileCommandIntegration::new();

        // Test executing commands
        assert!(integration.handle_notification_tile_click().is_ok());
        assert!(integration.handle_volume_change(0.75).is_ok());
        assert!(integration.handle_workspace_switch(3).is_ok());

        // Test undo functionality
        assert!(integration.can_undo());
        assert!(integration.undo_last_action().is_ok());
    }

    #[test]
    fn test_relm4_command_integration() {
        use relm4_integration::*;

        let mut component = ComponentWithCommands::new();

        // Test command execution
        component.process_command(CommandMsg::Execute(AppCommand::SetVolume(0.8)));
        assert!(component.last_command_result.is_some());

        // Test undo
        component.process_command(CommandMsg::Undo);
        assert!(component.last_command_result.is_some());
    }
}
