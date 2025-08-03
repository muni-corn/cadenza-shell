import { Astal, type Gdk, Gtk } from "ags/gtk4";
import { Battery } from "./battery";
import { Bluetooth } from "./bluetooth";
import { Brightness } from "./brightness";
import { Clock } from "./clock";
import { FocusedClient, Workspaces } from "./hyprland";
import { Media } from "./mpris";
import { Network } from "./network";
import { SysTray } from "./tray";
import { Volume } from "./volume";
import { Weather } from "./weather/index";

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
      <centerbox>
        <Left gdkmonitor={gdkmonitor} />
        <Center />
        <Right />
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

function Right() {
  return (
    <box spacing={20} halign={Gtk.Align.END}>
      <Brightness />
      <Volume />
      <Bluetooth />
      <Network />
      <Battery />
      <SysTray />
    </box>
  );
}
