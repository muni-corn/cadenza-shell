import { Astal, type Gdk, Gtk } from "ags/gtk4";
import { Battery } from "./battery";
import { Bluetooth } from "./bluetooth";
import { Brightness } from "./brightness";
import { Clock } from "./clock";
import { FocusedClient, Workspaces } from "./hyprland";
import { Media } from "./mpris";
import { Network } from "./network";
import { NotificationTile } from "./notifications/notification-tile";
import { SysTray } from "./tray";
import { Volume } from "./volume";
import { Weather } from "./weather/index";
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
    <box spacing={20} halign={Gtk.Align.END}>
      <Brightness />
      <Volume />
      <Bluetooth />
      <Network />
      <Battery />
      <NotificationTile gdkmonitor={gdkmonitor} />
      <SysTray />
    </box>
  );
}
