import GLib from "gi://GLib?version=2.0";
import { Gtk } from "ags/gtk4";
import { createPoll } from "ags/time";

export function AnalogClock() {
  const time = createPoll(
    {
      hours: 0,
      minutes: 0,
      seconds: 0,
    },
    1000,
    () => {
      const now = GLib.DateTime.new_now_local();
      return {
        hours: now.get_hour() % 12,
        minutes: now.get_minute(),
        seconds: now.get_second(),
      };
    },
  );

  return (
    <box
      class="analog-clock"
      orientation={Gtk.Orientation.VERTICAL}
      spacing={8}
      halign={Gtk.Align.CENTER}
    >
      <drawingarea
        class="analog-clock-face"
        widthRequest={180}
        heightRequest={180}
        $={(self) => {
          // Subscribe to time changes and trigger redraw
          time.subscribe(() => {
            self.queue_draw();
          });

          self.set_draw_func((_, cr, width, height) => {
            const centerX = width / 2;
            const centerY = height / 2;
            const radius = Math.min(width, height) / 2 - 10;

            const currentTime = time.get();

            // Clear the area
            cr.setSourceRGBA(0, 0, 0, 0);
            cr.paint();

            // Draw hour markers
            cr.setSourceRGBA(1, 1, 1, 0.5);
            for (let i = 0; i < 12; i++) {
              const angle = (i * Math.PI) / 6 - Math.PI / 2;
              const x = centerX + Math.cos(angle) * (radius - 5);
              const y = centerY + Math.sin(angle) * (radius - 5);

              cr.arc(x, y, 2, 0, 2 * Math.PI);
              cr.fill();
            }

            // Draw hour hand
            const hourAngle =
              ((currentTime.hours + currentTime.minutes / 60) * Math.PI) / 6 -
              Math.PI / 2;
            cr.setSourceRGB(1, 1, 1);
            cr.setLineWidth(4);
            cr.setLineCap(1); // Round cap
            cr.moveTo(centerX, centerY);
            cr.lineTo(
              centerX + Math.cos(hourAngle) * (radius * 0.5),
              centerY + Math.sin(hourAngle) * (radius * 0.5),
            );
            cr.stroke();

            // Draw minute hand
            const minuteAngle =
              (currentTime.minutes * Math.PI) / 30 - Math.PI / 2;
            cr.setSourceRGBA(1, 1, 1, 0.75);
            cr.setLineWidth(3);
            cr.setLineCap(1); // Round cap
            cr.moveTo(centerX, centerY);
            cr.lineTo(
              centerX + Math.cos(minuteAngle) * (radius * 0.75),
              centerY + Math.sin(minuteAngle) * (radius * 0.75),
            );
            cr.stroke();

            // Draw second hand
            const secondAngle =
              (currentTime.seconds * Math.PI) / 30 - Math.PI / 2;
            cr.setSourceRGBA(1, 1, 1, 0.5);
            cr.setLineWidth(1);
            cr.setLineCap(1); // Round cap
            cr.moveTo(centerX, centerY);
            cr.lineTo(
              centerX + Math.cos(secondAngle) * (radius * 0.85),
              centerY + Math.sin(secondAngle) * (radius * 0.85),
            );
            cr.stroke();

            return false;
          });
        }}
      />
    </box>
  );
}
