import type { Binding } from "astal";
import { type Gdk, Gtk } from "astal/gtk3";
import { ProgressBar } from "./progress";

export type SingleMonitorProps = { gdkmonitor: Gdk.Monitor };

export enum Attention {
  Alarm = "alarm",
  Warning = "warning",
  Normal = "",
}

export interface Tile {
  icon: string;
  primary: string;
  secondary: string;
  visible?: boolean;
  attention?: Attention;
}

export function makeTile(data: Binding<Tile>): JSX.Element {
  const className = (otherClasses = "") =>
    data.as((d) =>
      d.attention ? `${otherClasses} ${d.attention}` : otherClasses,
    );

  const icon = data.as((d) => trunc(d.icon));
  const primary = data.as((d) => trunc(d.primary));
  const secondary = data.as((d) => trunc(d.secondary));
  const visible = data.as((d) => d.visible ?? true);

  return (
    <box spacing={12} visible={visible}>
      <label
        label={icon}
        visible={icon.as((p) => p.length > 0)}
        className={className("icon")}
        widthRequest={16}
      />
      <label
        label={primary}
        visible={primary.as((p) => p.length > 0)}
        className={className("primary")}
      />
      <label
        label={secondary}
        visible={secondary.as((s) => (s && s.length > 0) || false)}
        className={className("secondary")}
      />
    </box>
  );
}

export interface ProgressTile {
  icon: string;
  progress: number;
  visible?: boolean;
}

export function makeProgressTile(data: Binding<ProgressTile>) {
  const icon = data.as((d) => trunc(d.icon));
  const progress = data.as((d) => d.progress);
  const visible = data.as((d) => d.visible ?? true);

  return (
    <box spacing={8} visible={visible}>
      <label
        label={icon}
        visible={icon.as((p) => p.length > 0)}
        className={"icon dim"}
        widthRequest={16}
      />
      <ProgressBar fraction={progress} valign={Gtk.Align.CENTER} />
    </box>
  );
}

/** Returns an icon from a list based on a percentage from 0 to 1. */
export function percentageToIconFromList(percentage: number, icons: string[]) {
  const listLength = icons.length;
  const index = Math.min(listLength - 1, Math.floor(listLength * percentage));
  return icons[index];
}

export function trunc(s: string, n = 32) {
  return s && s.length > n ? `${s.slice(0, n)}…` : s || "";
}

export function unreachable(_: never): never {
  throw new Error("unreachable case reached");
}
