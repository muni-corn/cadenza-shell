use std::{
    cell::{Cell, RefCell},
    fmt,
    rc::Rc,
};

use glib::ControlFlow;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, LayerShell};

/// Margin between the panel and the screen edges while it's fully shown.
pub const PANEL_MARGIN: i32 = 8;

/// Time a full slide between the hidden and shown positions takes.
const SLIDE_DURATION_US: f64 = 500_000.0;

/// Width of the window that stays on screen while it's slid out.
///
/// The compositor stops sending frame callbacks to a layer surface that's
/// entirely off-screen, which stalls the animation, so a sliver is kept
/// visible until the window is unmapped.
const HIDDEN_SLIVER: i32 = 1;

/// Opacity used on alternating frames to force a repaint.
const REPAINT_OPACITY: f64 = 0.996;

struct State {
    target_shown: bool,
    // current right margin; negative while the window is partially off-screen
    margin: f64,
    tick: Option<gtk4::TickCallbackId>,
}

/// Slides a right-anchored layer-shell window in and out by animating its
/// right margin.
///
/// Moving the surface itself, instead of animating its contents, means
/// compositor-drawn effects like shadows move with the panel, and nothing is
/// left behind once the window is unmapped.
pub struct PanelSlide {
    window: gtk4::Window,
    state: Rc<RefCell<State>>,
}

impl fmt::Debug for PanelSlide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PanelSlide").finish_non_exhaustive()
    }
}

impl PanelSlide {
    /// Creates a slide controller for a window that starts out hidden.
    pub fn new(window: &gtk4::Window) -> Self {
        Self {
            window: window.clone(),
            state: Rc::new(RefCell::new(State {
                target_shown: false,
                margin: f64::from(PANEL_MARGIN),
                tick: None,
            })),
        }
    }

    /// Maps the window and slides it in from the right edge of the screen.
    ///
    /// Does nothing if the window is already shown or sliding in.
    pub fn slide_in(&self) {
        {
            let mut state = self.state.borrow_mut();
            if state.target_shown {
                return;
            }
            state.target_shown = true;

            // a fully hidden window starts off-screen; one that's mid slide-out
            // reverses from wherever it currently is
            if !self.window.is_visible() {
                state.margin = self.hidden_margin();
                self.window
                    .set_margin(Edge::Right, state.margin.round() as i32);
                self.window.set_visible(true);
            }
        }
        self.animate_to(f64::from(PANEL_MARGIN));
    }

    /// Slides the window off the right edge of the screen, then unmaps it.
    ///
    /// Unmapping destroys the layer surface, which also removes anything the
    /// compositor draws for it, such as a shadow.
    ///
    /// Does nothing if the window is already hidden or sliding out.
    pub fn slide_out(&self) {
        {
            let mut state = self.state.borrow_mut();
            if !state.target_shown {
                return;
            }
            state.target_shown = false;
        }
        self.animate_to(self.hidden_margin());
    }

    /// Right margin that leaves only a sliver of the window on screen.
    fn hidden_margin(&self) -> f64 {
        // the natural width is known before the window is first allocated
        let (_, natural, ..) = self.window.measure(gtk4::Orientation::Horizontal, -1);
        let width = natural.max(self.window.width());
        -f64::from(width - HIDDEN_SLIVER)
    }

    fn animate_to(&self, to: f64) {
        let mut state = self.state.borrow_mut();
        if let Some(tick) = state.tick.take() {
            tick.remove();
        }

        let from = state.margin;
        let full_distance = (f64::from(PANEL_MARGIN) - self.hidden_margin()).abs();
        // a reversed slide only covers the remaining distance, so it shouldn't
        // take a full duration
        let duration = SLIDE_DURATION_US * ((to - from).abs() / full_distance).clamp(0.0, 1.0);

        let shared = Rc::clone(&self.state);
        let start_time = Cell::new(None::<i64>);
        let frame = Cell::new(0u32);
        let tick = self.window.add_tick_callback(move |window, clock| {
            let now = clock.frame_time();
            let start = start_time.get().unwrap_or_else(|| {
                start_time.set(Some(now));
                now
            });

            let progress = if duration <= 0.0 {
                1.0
            } else {
                ((now - start) as f64 / duration).min(1.0)
            };
            let margin = from + (to - from) * ease_out_expo(progress);
            window.set_margin(Edge::Right, margin.round() as i32);

            let finished = progress >= 1.0;
            let target_shown = {
                let mut state = shared.borrow_mut();
                state.margin = margin;
                if finished {
                    // returning Break removes the callback, so the id must
                    // not be removed again
                    state.tick = None;
                }
                state.target_shown
            };

            if !finished {
                frame.set(frame.get() + 1);
                force_repaint(window, frame.get());
                return ControlFlow::Continue;
            }

            restore_opacity(window);
            if !target_shown {
                window.set_visible(false);
            }
            ControlFlow::Break
        });
        state.tick = Some(tick);
    }
}

/// Forces the window to repaint so the compositor paces the animation.
///
/// Moving a layer surface by its margin doesn't change what's drawn, so GTK
/// has nothing to paint, never requests a frame callback, and ticks at a
/// fixed 60 Hz regardless of the display's refresh rate. Nudging the opacity
/// by an imperceptible amount gives every frame real damage, which ties
/// each tick to the compositor's frame callbacks.
fn force_repaint(window: &gtk4::Window, frame: u32) {
    if let Some(child) = window.child() {
        let opacity = if frame % 2 == 0 { 1.0 } else { REPAINT_OPACITY };
        child.set_opacity(opacity);
    }
}

fn restore_opacity(window: &gtk4::Window) {
    if let Some(child) = window.child() {
        child.set_opacity(1.0);
    }
}

/// Decelerating easing curve for progress in the range 0 to 1.
fn ease_out_expo(t: f64) -> f64 {
    // the curve only approaches 1, so it's pinned there to land exactly
    if t >= 1.0 {
        1.0
    } else {
        1.0 - 2f64.powf(-10.0 * t)
    }
}
