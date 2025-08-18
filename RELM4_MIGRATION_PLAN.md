# Relm4 Migration Plan for muse-shell

## Overview
This document provides a comprehensive step-by-step plan to refactor the muse-shell codebase from raw GTK4 to Relm4, a more idiomatic Rust GUI framework that simplifies development with the Model-View-Update pattern.

## Phase 1: Setup and Core Infrastructure (Priority: HIGH)

### Step 1: Add Relm4 Dependencies
**File:** `Cargo.toml`
**Action:** Add the following dependencies:
```toml
# Core Relm4
relm4 = { version = "0.9", features = ["tokio-rt", "macros"] }
relm4-components = "0.9"
relm4-icons = "0.9.0"

# Keep existing GTK dependencies as they're compatible
```

### Step 2: Create Main Application Component
**File:** `src/app_relm4.rs` (new file)
**Action:** Create a new Relm4 application structure:
1. Define `AppModel` struct containing:
   - List of bar windows
   - Global application state
   - Service handles
2. Define `AppMsg` enum for application-level messages:
   - `MonitorAdded(Monitor)`
   - `MonitorRemoved(Monitor)`
   - `Quit`
3. Implement `SimpleComponent` for the main app
4. Use `RelmApp::new()` instead of GTK `Application::new()`
5. Handle multi-monitor setup in the `init` method

### Step 3: Refactor Bar Widget to Relm4 Component
**File:** `src/widgets/bar_relm4.rs` (new file)
**Action:** Convert Bar to a Relm4 component:
1. Define `BarModel` struct:
   - `monitor: Monitor`
   - `left_tiles: Vec<Controller<TileComponent>>`
   - `center_tiles: Vec<Controller<TileComponent>>`
   - `right_tiles: Vec<Controller<TileComponent>>`
2. Define `BarMsg` enum:
   - `UpdateTiles`
   - `TileClicked(TileId)`
3. Use `view!` macro to define the bar layout declaratively
4. Implement factory pattern for tiles

## Phase 2: Service Layer Refactoring (Priority: HIGH)

### Step 4: Create Relm4 Workers for Services
**Files:** `src/services/*_worker.rs` (new files)
**Action:** For each service (battery, network, brightness, etc.):
1. Create a `Worker` implementation using Relm4's `Worker` trait
2. Define service-specific messages (Input/Output)
3. Move polling/monitoring logic to worker's `update` method
4. Use channels for communication with components
5. Example for BatteryWorker:
   ```rust
   struct BatteryWorker {
       percentage: f64,
       charging: bool,
       available: bool,
   }
   
   enum BatteryInput {
       Poll,
       UpdateInterval(Duration),
   }
   
   enum BatteryOutput {
       StateChanged { percentage: f64, charging: bool },
       BatteryUnavailable,
   }
   ```

### Step 5: Implement Component Communication
**File:** `src/messages.rs` (new file)
**Action:** Create a centralized message bus:
1. Define global message types for inter-component communication
2. Create a `MessageBus` struct using Relm4's `Sender`/`Receiver`
3. Implement subscription pattern for components

## Phase 3: Tile Components Migration (Priority: MEDIUM)

### Step 6-15: Convert Each Tile to Relm4 SimpleComponent
For each tile widget, follow this pattern:

**Example for Battery Tile:**
**File:** `src/tiles/battery_relm4.rs`
```rust
struct BatteryTile {
    percentage: f64,
    charging: bool,
    visible: bool,
    attention: Attention,
}

#[derive(Debug)]
enum BatteryMsg {
    UpdateState(f64, bool),
    Click,
    ShowDetails,
}

#[relm4::component]
impl SimpleComponent for BatteryTile {
    type Init = ();
    type Input = BatteryMsg;
    type Output = TileOutput;
    
    view! {
        gtk::Box {
            add_css_class: "tile",
            add_css_class: "battery",
            #[watch]
            set_visible: model.visible,
            
            gtk::Image {
                #[watch]
                set_icon_name: Some(&model.get_icon()),
            },
            
            gtk::Label {
                #[watch]
                set_text: &format!("{}%", (model.percentage * 100.0) as u32),
            },
            
            connect_clicked[sender] => move |_| {
                sender.input(BatteryMsg::Click);
            }
        }
    }
    
    fn init(...) -> ComponentParts<Self> { ... }
    fn update(...) { ... }
}
```

**Tiles to Convert (in order):**
1. Battery (`src/tiles/battery_relm4.rs`)
2. Bluetooth (`src/tiles/bluetooth_relm4.rs`)
3. Brightness (`src/tiles/brightness_relm4.rs`)
4. Clock (`src/tiles/clock_relm4.rs`)
5. Hyprland Workspaces (`src/tiles/hyprland_relm4.rs`)
6. Network (`src/tiles/network_relm4.rs`)
7. Volume (`src/tiles/volume_relm4.rs`)
8. Notifications (`src/tiles/notifications_relm4.rs`)
9. MPRIS (`src/tiles/mpris_relm4.rs`)
10. System Tray (`src/tiles/tray_relm4.rs`)

## Phase 4: Complex Components (Priority: HIGH/MEDIUM)

### Step 11: Implement WiFi Menu Component
**File:** `src/components/wifi_menu.rs` (new file)
**Action:** Migrate TypeScript WiFi menu to Rust:
1. Create `WiFiMenuModel`:
   - `access_points: FactoryVecDeque<AccessPoint>`
   - `scanning: bool`
   - `enabled: bool`
   - `current_ssid: Option<String>`
2. Use Relm4's factory pattern for access point list
3. Implement password dialog as a separate component
4. Handle async connection operations with `Command`

### Step 12: Convert Weather Tile to AsyncComponent
**File:** `src/tiles/weather_relm4.rs`
**Action:** Use `AsyncComponent` for weather API calls:
1. Implement async weather fetching in `update`
2. Use `Command` for non-blocking HTTP requests
3. Add proper error handling with fallback UI

### Steps 16-18: Refactor Notification System
**Files:** `src/notifications/*_relm4.rs`
**Action:** 
1. **NotificationCenter**: Use `FactoryVecDeque` for notification list
2. **NotificationCard**: Implement as `FactoryComponent`
3. **NotificationPopup**: Use Relm4's transient windows
4. Implement notification actions with message passing

### Step 19: Convert Analog Clock
**File:** `src/widgets/analog_clock_relm4.rs`
**Action:** Use Relm4's `DrawingHandler`:
1. Implement custom drawing logic in `DrawingHandler`
2. Use `draw_handler.emit_draw()` for updates
3. Connect to time service for updates

## Phase 5: Application Features (Priority: MEDIUM/LOW)

### Step 22: Update CSS Loading
**File:** `src/style/mod.rs`
**Action:** 
1. Load CSS in Relm4's `setup` method
2. Use `relm4::set_global_css()` for application-wide styles
3. Support hot-reloading in debug builds

### Step 23: Implement Settings Management
**File:** `src/settings.rs` (new file)
**Action:**
1. Create `Settings` component with Relm4
2. Use GSettings or config file backend
3. Implement settings dialog UI

### Step 24: Add Command Pattern
**File:** `src/commands.rs` (new file)
**Action:**
1. Define `Command` trait for user actions
2. Implement undo/redo support
3. Add keyboard shortcuts with Relm4's actions

## Phase 6: Testing and Documentation (Priority: LOW)

### Step 25: Test Multi-Monitor Support
**Action:**
1. Test bar creation/destruction on monitor changes
2. Verify window positioning with gtk4-layer-shell
3. Test DPI scaling across different monitors

### Step 26: Update Documentation
**Files:** `AGENTS.md`, `README.md`, etc.
**Action:**
1. Update build instructions for Relm4
2. Document new component architecture
3. Add examples for extending components
4. Update development workflow

## Implementation Guidelines for LLM

When implementing each step:

1. **Preserve Existing Functionality**: Keep the old implementation alongside the new one until fully tested
2. **Use Relm4 Patterns**:
   - Prefer `view!` macro over manual widget construction
   - Use `#[watch]` for reactive updates
   - Implement `Worker` for background tasks
   - Use `Command` for async operations
   - Apply factory pattern for dynamic lists

3. **Component Structure**:
   ```rust
   // Each component should follow this structure:
   mod model {
       pub struct ComponentModel { /* fields */ }
   }
   
   mod messages {
       pub enum ComponentInput { /* variants */ }
       pub enum ComponentOutput { /* variants */ }
   }
   
   mod widgets {
       // view! macro implementation
   }
   
   impl SimpleComponent for ComponentModel {
       // Implementation
   }
   ```

4. **Service Integration**:
   - Services should be Workers that emit messages
   - Components subscribe to service messages
   - Use weak references to avoid circular dependencies

5. **Error Handling**:
   - Use `anyhow::Result` for fallible operations
   - Show user-friendly error notifications
   - Log errors for debugging

6. **Testing Strategy**:
   - Test each component in isolation first
   - Use `cargo run --example component_name` for testing
   - Implement integration tests for service-component communication

## Migration Order Summary

1. **Week 1**: Core infrastructure (Steps 1-3)
2. **Week 2**: Service workers and message bus (Steps 4-5)
3. **Week 3-4**: Basic tiles (Steps 6-10)
4. **Week 5**: WiFi menu and complex components (Steps 11, 16-18)
5. **Week 6**: Remaining tiles and features (Steps 12-15, 19, 22-24)
6. **Week 7**: Testing and documentation (Steps 25-26)

## Success Criteria

- All tiles display correctly and update in real-time
- Services communicate efficiently through message passing
- Memory usage is reduced compared to raw GTK implementation
- Code is more maintainable and follows Rust idioms
- Multi-monitor support works reliably
- All TypeScript components are successfully migrated to Rust

## Notes for Implementation

- Start with a single tile (e.g., Battery) as a proof of concept
- Keep both implementations running in parallel during migration
- Use feature flags to switch between old and new implementations
- Regular testing on actual hardware (not just in VM)
- Consider using `relm4-template` for boilerplate generation
