import type { Accessor } from "ags";
import { type Gdk, Gtk } from "ags/gtk4";

export type SingleMonitorProps = { gdkmonitor: Gdk.Monitor };

export enum Attention {
  Alarm = "alarm",
  Warning = "warning",
  Normal = "",
  Dim = "dim",
}

export interface TileProps {
  icon?: string | Accessor<string>;
  primary?: string | Accessor<string>;
  secondary?: string | Accessor<string>;
  visible?: boolean | Accessor<boolean>;
  attention?: Attention | Accessor<Attention>;
}

export const Tile = ({
  icon,
  primary,
  secondary,
  visible,
  attention,
}: TileProps) => {
  const className = (otherClasses: string[] = []) => {
    if (!attention) return otherClasses;
    if (typeof attention === "string") {
      return attention ? otherClasses.concat([attention]) : otherClasses;
    }
    return attention.as((a) => (a ? otherClasses.concat([a]) : otherClasses));
  };

  const iconLabel = icon
    ? typeof icon === "string"
      ? trunc(icon)
      : icon.as((i) => trunc(i || ""))
    : "";
  const primaryLabel = primary
    ? typeof primary === "string"
      ? trunc(primary)
      : primary.as((p) => trunc(p || ""))
    : "";
  const secondaryLabel = secondary
    ? typeof secondary === "string"
      ? trunc(secondary)
      : secondary.as((s) => trunc(s || ""))
    : "";
  const isVisible =
    visible !== undefined
      ? typeof visible === "boolean"
        ? visible
        : visible.as((v) => v ?? true)
      : true;

  return (
    <box spacing={12} visible={isVisible}>
      <label
        label={iconLabel}
        visible={
          typeof iconLabel === "string"
            ? iconLabel.length > 0
            : iconLabel.as((p) => p.length > 0)
        }
        cssClasses={className(["icon"])}
        widthRequest={16}
      />
      <label
        label={primaryLabel}
        visible={
          typeof primaryLabel === "string"
            ? primaryLabel.length > 0
            : primaryLabel.as((p) => p.length > 0)
        }
        cssClasses={className(["primary"])}
      />
      <label
        label={secondaryLabel}
        visible={
          typeof secondaryLabel === "string"
            ? secondaryLabel.length > 0
            : secondaryLabel.as((s) => s?.length > 0)
        }
        cssClasses={className(["secondary"])}
      />
    </box>
  );
};

export interface ProgressTile {
  icon: string;
  progress: number;
  visible?: boolean;
}

export const ProgressTile = ({ data }: { data: Accessor<ProgressTile> }) => {
  const icon = data.as((d) => trunc(d.icon));
  const progress = data.as((d) => d.progress);
  const visible = data.as((d) => d.visible ?? true);

  return (
    <box spacing={8} visible={visible}>
      <label
        label={icon}
        visible={icon.as((p) => p.length > 0)}
        cssClasses={["icon", "dim"]}
        widthRequest={16}
      />
      <Gtk.ProgressBar fraction={progress} valign={Gtk.Align.CENTER} />
    </box>
  );
};

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
