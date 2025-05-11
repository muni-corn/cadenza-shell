import { App, Astal, type Gdk, Gtk } from "astal/gtk4";
import { Clock } from "./clock.tsx";
import { SysTray } from "./tray.tsx";
import { Battery } from "./battery.tsx";
import { Network } from "./network.tsx";
import { Bluetooth } from "./bluetooth.tsx";
import { Volume } from "./volume.tsx";
import { Brightness } from "./brightness.tsx";
import { Media } from "./mpris.tsx";
import { Weather } from "./weather/index.ts";
import { FocusedClient, Workspaces } from "./hyprland.tsx";

export function Bar(gdkmonitor: Gdk.Monitor) {
  return (
    <window
      visible={true}
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
      application={App}
    >
      <centerbox>
        <Left gdkmonitor={gdkmonitor} />
        <Center />
        <Right />
      </centerbox>
    </window>
  );
}

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
