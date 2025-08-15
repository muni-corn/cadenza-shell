import { Astal, type Gdk, Gtk } from "ags/gtk4";
import { Battery } from "./tiles/battery";
import { Bluetooth } from "./tiles/bluetooth";
import { Brightness } from "./tiles/brightness";
import { Clock } from "./tiles/clock";
import { FocusedClient, Workspaces } from "./tiles/hyprland";
import { Media } from "./tiles/mpris";
import { Network } from "./tiles/network";
import { NotificationTile } from "./tiles/notifications";
import { SysTray } from "./tiles/tray";
import { Volume } from "./tiles/volume";
import { Weather } from "./tiles/weather/index";
import type { SingleMonitorProps } from "./utils";

export const Bar = (gdkmonitor: Gdk.Monitor) => {
  return (
    <window
      visible
      cssClasses={["bar"]}
      namespace="bar"
      gdkmonitor={gdkmonitor}
      exclusivity={Astal.Exclusivity.EXCLUSIVE}
      anchor={
        Astal.WindowAnchor.TOP |
        Astal.WindowAnchor.LEFT |
        Astal.WindowAnchor.RIGHT
      }
      heightRequest={32}
    >
      <centerbox
        orientation={Gtk.Orientation.HORIZONTAL}
        shrink_center_last
        class="bar-groups"
      >
        <Left gdkmonitor={gdkmonitor} $type="start" />
        <Center $type="center" />
        <Right gdkmonitor={gdkmonitor} $type="end" />
      </centerbox>
    </window>
  );
};

// layout of the bar
function Left({ gdkmonitor }: { gdkmonitor: Gdk.Monitor }) {
  return (
    <box spacing={20} cssClasses={["workspaces"]}>
      <Workspaces gdkmonitor={gdkmonitor} />
      <FocusedClient gdkmonitor={gdkmonitor} />
    </box>
  );
}

function Center() {
  return (
    <box spacing={20} halign={Gtk.Align.START}>
      <Clock />
      <Weather />
      <Media />
    </box>
  );
}

function Right({ gdkmonitor }: SingleMonitorProps) {
  return (
    <box spacing={20}>
      <box spacing={20} halign={Gtk.Align.END}>
        <Brightness />
        <Volume />
        <Bluetooth />
        <Network />
        <Battery />
      </box>
      <box>
        <SysTray />
        <NotificationTile gdkmonitor={gdkmonitor} />
      </box>
    </box>
  );
}
