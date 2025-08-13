import GLib from "gi://GLib?version=2.0";
import { createPoll } from "ags/time";

export function AnalogClock({ radius = 50 }: { radius?: number }) {
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
        minutes: now.get_minute() + now.get_second() / 60,
        seconds: now.get_second(),
      };
    },
  );

  return (
    <drawingarea
      class="analog-clock-face"
      widthRequest={radius * 2}
      heightRequest={radius * 2}
      $={(self) => {
        // Subscribe to time changes and trigger redraw
        time.subscribe(() => {
          self.queue_draw();
        });

        self.set_draw_func((_, cr, width, height) => {
          const centerX = width / 2;
          const centerY = height / 2;

          const currentTime = time.get();

          // Clear the area
          cr.setSourceRGBA(0, 0, 0, 0);
          cr.paint();

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
            centerX + Math.cos(secondAngle) * radius,
            centerY + Math.sin(secondAngle) * radius,
          );
          cr.stroke();

          return false;
        });
      }}
    />
  );
}
