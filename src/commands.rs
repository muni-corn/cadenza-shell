use std::collections::VecDeque;

pub mod integration;

/// Trait for commands that can be executed and undone
pub trait Command: std::fmt::Debug + Send {
    /// Execute the command
    fn execute(&self) -> Result<(), String>;

    /// Undo the command (if possible)
    fn undo(&self) -> Result<(), String> {
        Err("undo not supported for this command".to_string())
    }

    /// Get a human-readable description of the command
    fn description(&self) -> String;

    /// Whether this command can be undone
    fn can_undo(&self) -> bool {
        false
    }
}

/// Command manager that handles execution and undo/redo functionality
#[derive(Debug)]
pub struct CommandManager {
    undo_stack: VecDeque<Box<dyn Command>>,
    redo_stack: VecDeque<Box<dyn Command>>,
    max_history_size: usize,
}

impl CommandManager {
    pub fn new(max_history_size: usize) -> Self {
        Self {
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            max_history_size,
        }
    }

    /// Execute a command and add it to the undo stack if it supports undo
    pub fn execute_command(&mut self, command: Box<dyn Command>) -> Result<(), String> {
        let result = command.execute();

        if result.is_ok() && command.can_undo() {
            // clear redo stack when a new command is executed
            self.redo_stack.clear();

            // add to undo stack
            self.undo_stack.push_back(command);

            // limit history size
            if self.undo_stack.len() > self.max_history_size {
                self.undo_stack.pop_front();
            }
        }

        result
    }

    /// Undo the last command
    pub fn undo(&mut self) -> Result<String, String> {
        if let Some(command) = self.undo_stack.pop_back() {
            let description = command.description();
            match command.undo() {
                Ok(()) => {
                    self.redo_stack.push_back(command);
                    Ok(format!("Undid: {}", description))
                }
                Err(e) => {
                    // put the command back if undo failed
                    self.undo_stack.push_back(command);
                    Err(e)
                }
            }
        } else {
            Err("Nothing to undo".to_string())
        }
    }

    /// Redo the last undone command
    pub fn redo(&mut self) -> Result<String, String> {
        if let Some(command) = self.redo_stack.pop_back() {
            let description = command.description();
            match command.execute() {
                Ok(()) => {
                    self.undo_stack.push_back(command);
                    Ok(format!("Redid: {}", description))
                }
                Err(e) => {
                    // put the command back if redo failed
                    self.redo_stack.push_back(command);
                    Err(e)
                }
            }
        } else {
            Err("Nothing to redo".to_string())
        }
    }

    /// Check if there are commands that can be undone
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if there are commands that can be redone
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Get the description of the next command that would be undone
    pub fn next_undo_description(&self) -> Option<String> {
        self.undo_stack.back().map(|cmd| cmd.description())
    }

    /// Get the description of the next command that would be redone
    pub fn next_redo_description(&self) -> Option<String> {
        self.redo_stack.back().map(|cmd| cmd.description())
    }

    /// Clear all command history
    pub fn clear_history(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

/// Application-specific commands for muse-shell
#[derive(Debug)]
pub enum AppCommand {
    /// Toggle notification center visibility
    ToggleNotificationCenter,
    /// Dismiss a specific notification
    DismissNotification(u32),
    /// Toggle wifi menu
    ToggleWifiMenu,
    /// Switch to workspace
    SwitchWorkspace(u32),
    /// Adjust volume
    SetVolume(f64),
    /// Adjust brightness
    SetBrightness(f64),
    /// Toggle media playback
    ToggleMediaPlayback,
}

impl Command for AppCommand {
    fn execute(&self) -> Result<(), String> {
        match self {
            AppCommand::ToggleNotificationCenter => {
                log::info!("toggling notification center");
                // in a real implementation, this would send a message to the notification
                // center component
                Ok(())
            }
            AppCommand::DismissNotification(id) => {
                log::info!("dismissing notification {}", id);
                // in a real implementation, this would interact with the notification service
                Ok(())
            }
            AppCommand::ToggleWifiMenu => {
                log::info!("toggling WiFi menu");
                Ok(())
            }
            AppCommand::SwitchWorkspace(workspace) => {
                log::info!("switching to workspace {}", workspace);
                // this would interact with Hyprland service
                Ok(())
            }
            AppCommand::SetVolume(volume) => {
                log::info!("setting volume to {}", volume);
                // this would interact with audio service
                Ok(())
            }
            AppCommand::SetBrightness(brightness) => {
                log::info!("setting brightness to {}", brightness);
                // this would interact with brightness service
                Ok(())
            }
            AppCommand::ToggleMediaPlayback => {
                log::info!("toggling media playback");
                // this would interact with MPRIS service
                Ok(())
            }
        }
    }

    fn undo(&self) -> Result<(), String> {
        match self {
            // some commands can be undone
            AppCommand::SwitchWorkspace(_) => {
                log::info!("undoing workspace switch (return to previous)");
                Ok(())
            }
            AppCommand::SetVolume(_) => {
                log::info!("undoing volume change (restore previous level)");
                Ok(())
            }
            AppCommand::SetBrightness(_) => {
                log::info!("undoing brightness change (restore previous level)");
                Ok(())
            }
            // others cannot be meaningfully undone
            _ => Err("this command cannot be undone".to_string()),
        }
    }

    fn description(&self) -> String {
        match self {
            AppCommand::ToggleNotificationCenter => "toggle notification center".to_string(),
            AppCommand::DismissNotification(id) => format!("dismiss notification {}", id),
            AppCommand::ToggleWifiMenu => "toggle wifi menu".to_string(),
            AppCommand::SwitchWorkspace(ws) => format!("switch to workspace {}", ws),
            AppCommand::SetVolume(vol) => format!("set volume to {:.0}%", vol * 100.0),
            AppCommand::SetBrightness(br) => format!("set brightness to {:.0}%", br * 100.0),
            AppCommand::ToggleMediaPlayback => "toggle media playback".to_string(),
        }
    }

    fn can_undo(&self) -> bool {
        matches!(
            self,
            AppCommand::SwitchWorkspace(_)
                | AppCommand::SetVolume(_)
                | AppCommand::SetBrightness(_)
        )
    }
}

/// Global command executor that can be used across the application
#[derive(Debug)]
pub struct GlobalCommandExecutor {
    command_manager: CommandManager,
}

impl GlobalCommandExecutor {
    pub fn new() -> Self {
        Self {
            command_manager: CommandManager::new(50), // Keep last 50 commands
        }
    }

    pub fn execute(&mut self, command: AppCommand) -> Result<(), String> {
        // convert to Box<dyn Command>
        let cmd: Box<dyn Command> = Box::new(command);
        self.command_manager.execute_command(cmd)
    }

    pub fn undo(&mut self) -> Result<String, String> {
        self.command_manager.undo()
    }

    pub fn redo(&mut self) -> Result<String, String> {
        self.command_manager.redo()
    }

    pub fn can_undo(&self) -> bool {
        self.command_manager.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.command_manager.can_redo()
    }

    pub fn clear_history(&mut self) {
        self.command_manager.clear_history();
    }
}

impl Default for GlobalCommandExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_manager_execute_and_undo() {
        let mut manager = CommandManager::new(10);

        // test executing a command that can be undone
        let command = Box::new(AppCommand::SetVolume(0.5));
        assert!(manager.execute_command(command).is_ok());
        assert!(manager.can_undo());
        assert!(!manager.can_redo());

        // test undo
        let undo_result = manager.undo();
        assert!(undo_result.is_ok());
        assert!(!manager.can_undo());
        assert!(manager.can_redo());

        // test redo
        let redo_result = manager.redo();
        assert!(redo_result.is_ok());
        assert!(manager.can_undo());
        assert!(!manager.can_redo());
    }

    #[test]
    fn test_command_descriptions() {
        let volume_cmd = AppCommand::SetVolume(0.75);
        assert_eq!(volume_cmd.description(), "set volume to 75%");

        let workspace_cmd = AppCommand::SwitchWorkspace(3);
        assert_eq!(workspace_cmd.description(), "switch to workspace 3");
    }
}
