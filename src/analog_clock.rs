use crate::settings;
use chrono::Timelike;
use gtk4::DrawingArea;
use gtk4::glib;
use gtk4::prelude::*;
use std::f64::consts::PI;

pub struct AnalogClock {
    drawing_area: DrawingArea,
    radius: i32,
}

impl AnalogClock {
    pub fn new(radius: i32) -> Self {
        let drawing_area = DrawingArea::builder()
            .width_request(radius * 2)
            .height_request(radius * 2)
            .build();

        let clock = Self {
            drawing_area,
            radius,
        };

        clock.setup_drawing();
        clock.start_updates();

        clock
    }

    fn setup_drawing(&self) {
        let radius = self.radius as f64;

        self.drawing_area
            .set_draw_func(move |_, cr, width, height| {
                let center_x = width as f64 / 2.0;
                let center_y = height as f64 / 2.0;

                // Clear background
                cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
                cr.paint().unwrap();

                // Get current time
                let now = chrono::Local::now();
                let hours = now.hour() % 12;
                let minutes = now.minute();
                let seconds = now.second();

                // Draw clock face
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.1);
                cr.arc(center_x, center_y, radius * 0.9, 0.0, 2.0 * PI);
                cr.fill().unwrap();

                // Draw hour markers
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.8);
                cr.set_line_width(2.0);
                for i in 0..12 {
                    let angle = (i as f64) * PI / 6.0 - PI / 2.0;
                    let inner_radius = radius * 0.75;
                    let outer_radius = radius * 0.85;

                    let x1 = center_x + inner_radius * angle.cos();
                    let y1 = center_y + inner_radius * angle.sin();
                    let x2 = center_x + outer_radius * angle.cos();
                    let y2 = center_y + outer_radius * angle.sin();

                    cr.move_to(x1, y1);
                    cr.line_to(x2, y2);
                    cr.stroke().unwrap();
                }

                // Draw hour hand
                let hour_angle = (hours as f64 + minutes as f64 / 60.0) * PI / 6.0 - PI / 2.0;
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.9);
                cr.set_line_width(4.0);
                cr.move_to(center_x, center_y);
                cr.line_to(
                    center_x + radius * 0.5 * hour_angle.cos(),
                    center_y + radius * 0.5 * hour_angle.sin(),
                );
                cr.stroke().unwrap();

                // Draw minute hand
                let minute_angle = minutes as f64 * PI / 30.0 - PI / 2.0;
                cr.set_line_width(3.0);
                cr.move_to(center_x, center_y);
                cr.line_to(
                    center_x + radius * 0.7 * minute_angle.cos(),
                    center_y + radius * 0.7 * minute_angle.sin(),
                );
                cr.stroke().unwrap();

                // Draw second hand
                let second_angle = seconds as f64 * PI / 30.0 - PI / 2.0;
                cr.set_source_rgba(1.0, 0.3, 0.3, 0.8);
                cr.set_line_width(1.0);
                cr.move_to(center_x, center_y);
                cr.line_to(
                    center_x + radius * 0.8 * second_angle.cos(),
                    center_y + radius * 0.8 * second_angle.sin(),
                );
                cr.stroke().unwrap();

                // Draw center dot
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.9);
                cr.arc(center_x, center_y, 3.0, 0.0, 2.0 * PI);
                cr.fill().unwrap();
            });
    }

    fn start_updates(&self) {
        let drawing_area = self.drawing_area.clone();

        // Update every second
        glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
            drawing_area.queue_draw();
            glib::ControlFlow::Continue
        });
    }

    pub fn widget(&self) -> &DrawingArea {
        &self.drawing_area
    }
}