use crate::settings;
use chrono::Timelike;
use gtk4::{glib, prelude::*};
use relm4::{
    component::{AsyncComponent, AsyncComponentParts},
    gtk::{self, DrawingArea},
    AsyncComponentSender,
};
use std::{f64::consts::PI, time::Duration};

#[derive(Debug)]
pub struct AnalogClock {
    radius: f64,
}

#[derive(Debug)]
pub enum AnalogClockMsg {
    UpdateTime,
}

#[relm4::component(pub async)]
impl AsyncComponent for AnalogClock {
    type Init = f64;
    type Input = AnalogClockMsg;
    type Output = ();
    type CommandOutput = ();

    view! {
        #[root]
        #[name(drawing_area)]
        DrawingArea {
            set_width_request: (model.radius * 2.0) as i32,
            set_height_request: (model.radius * 2.0) as i32,

            set_draw_func: move |_, cr, width, height| {
                let center_x = width as f64 / 2.0;
                let center_y = height as f64 / 2.0;
                let radius = (width.min(height) as f64 / 2.0) * 0.9;

                // Clear background
                cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
                let _ = cr.paint();

                // Get current time
                let now = chrono::Local::now();
                let hours = now.hour() % 12;
                let minutes = now.minute();
                let seconds = now.second();
                let subseconds = now.nanosecond() as f64 / 1_000_000_000.0;

                // Draw hour markers (optional, subtle)
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.3);
                cr.set_line_width(1.0);
                for i in 0..12 {
                    let angle = (i as f64) * PI / 6.0 - PI / 2.0;
                    let inner_radius = radius * 0.85;
                    let outer_radius = radius * 0.95;

                    let x1 = center_x + inner_radius * angle.cos();
                    let y1 = center_y + inner_radius * angle.sin();
                    let x2 = center_x + outer_radius * angle.cos();
                    let y2 = center_y + outer_radius * angle.sin();

                    cr.move_to(x1, y1);
                    cr.line_to(x2, y2);
                    let _ = cr.stroke();
                }

                // Draw hour hand
                let hour_angle = (hours as f64 + minutes as f64 / 60.0) * PI / 6.0 - PI / 2.0;
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.9);
                cr.set_line_width(4.0);
                cr.set_line_cap(gtk4::cairo::LineCap::Round);
                cr.move_to(center_x, center_y);
                cr.line_to(
                    center_x + radius * 0.5 * hour_angle.cos(),
                    center_y + radius * 0.5 * hour_angle.sin(),
                );
                let _ = cr.stroke();

                // Draw minute hand
                let minute_angle = (minutes as f64 + seconds as f64 / 60.0) * PI / 30.0 - PI / 2.0;
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.75);
                cr.set_line_width(3.0);
                cr.set_line_cap(gtk4::cairo::LineCap::Round);
                cr.move_to(center_x, center_y);
                cr.line_to(
                    center_x + radius * 0.75 * minute_angle.cos(),
                    center_y + radius * 0.75 * minute_angle.sin(),
                );
                let _ = cr.stroke();

                // Draw second hand
                let second_angle = (seconds as f64 + subseconds) * PI / 30.0 - PI / 2.0;
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.5);
                cr.set_line_width(1.0);
                cr.set_line_cap(gtk4::cairo::LineCap::Round);
                cr.move_to(center_x, center_y);
                cr.line_to(
                    center_x + radius * 0.9 * second_angle.cos(),
                    center_y + radius * 0.9 * second_angle.sin(),
                );
                let _ = cr.stroke();

                // Draw center dot
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.9);
                cr.arc(center_x, center_y, 3.0, 0.0, 2.0 * PI);
                let _ = cr.fill();
            },
        }
    }

    async fn init(
        radius: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let model = AnalogClock { radius };

        let widgets = view_output!();

        // Start the timer for updates
        let sender_clone = sender.clone();
        glib::spawn_future_local(async move {
            let mut interval = glib::interval(Duration::from_millis(100));
            while let Some(_) = interval.next().await {
                let _ = sender_clone.input(AnalogClockMsg::UpdateTime);
            }
        });

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        message: Self::Input,
        _sender: AsyncComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            AnalogClockMsg::UpdateTime => {
                // Queue redraw
            }
        }
    }

    async fn update_view(&self, widgets: &mut Self::Widgets, _sender: AsyncComponentSender<Self>) {
        widgets.drawing_area.queue_draw();
    }
}