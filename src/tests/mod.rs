/// Test modules for muse-shell functionality
pub mod multi_monitor;

// Re-export commonly used test functions
pub use multi_monitor::integration_tests::run_all_tests as run_multi_monitor_tests;
pub use multi_monitor::run_monitor_diagnostics;
