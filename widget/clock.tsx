import { GLib, Variable } from "astal";
import { Tile } from "./utils.tsx";

const TIME_FORMAT = "%-I:%M %P";
const DATE_FORMAT = "%a, %b %-d";
const CLOCK_ICONS = [
  "\u{F1456}",
  "\u{F144B}",
  "\u{F144C}",
  "\u{F144D}",
  "\u{F144E}",
  "\u{F144F}",
  "\u{F1450}",
  "\u{F1451}",
  "\u{F1452}",
  "\u{F1453}",
  "\u{F1454}",
  "\u{F1455}",
];

export const Clock = () => {
  const date = Variable({
    icon: "",
    primary: "",
    secondary: getGreeting(),
  }).poll(1000, () => {
    const now = GLib.DateTime.new_now_local();
    const icon = CLOCK_ICONS[new Date().getHours() % 12];
    const time = now.format(TIME_FORMAT) || "invalid time format";
    const date = now.format(DATE_FORMAT) || "invalid date format";

    return {
      icon,
      primary: time,
      secondary: date,
    };
  });

  return <Tile data={date()} />;
};

export function getGreeting(): string {
  const hour = new Date().getHours();
  if (hour >= 3 && hour < 12) {
    return "Good morning";
  }
  if (hour >= 12 && hour < 17) {
    return "Good afternoon";
  }
  return "Good evening";
}
