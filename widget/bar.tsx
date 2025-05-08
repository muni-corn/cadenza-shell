import { App, Astal, type Gdk, Gtk } from "astal/gtk3";
import { Battery } from "./battery.tsx";
import { Bluetooth } from "./bluetooth.tsx";
import { Brightness } from "./brightness.tsx";
import { Clock } from "./clock.tsx";
import { FocusedClient, Workspaces } from "./hyprland.tsx";
import { Media } from "./mpris.tsx";
import { Network } from "./network.tsx";
import { SysTray } from "./tray.tsx";
import type { SingleMonitorProps } from "./utils.tsx";
import { Volume } from "./volume.tsx";
import { Weather } from "./weather/index.ts";

export function Bar(gdkmonitor: Gdk.Monitor) {
  return (
    <window
      className="bar"
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
      <centerbox spacing={40}>
        <Left gdkmonitor={gdkmonitor} />
        <Center />
        <Right />
      </centerbox>
    </window>
  );
}

// layout of the bar
function Left({ gdkmonitor }: SingleMonitorProps) {
  return (
    <box spacing={20} className="workspaces">
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
