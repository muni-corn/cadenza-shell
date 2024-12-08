import { App, Astal, Gdk, Gtk } from "astal/gtk3";
import { Clock } from "./clock";
import { Weather } from "./weather";
import { Media } from "./mpris";
import { Brightness } from "./brightness";
import { Volume } from "./volume";
import { Bluetooth } from "./bluetooth";
import { Network } from "./network";
import { Battery } from "./battery";
import { SysTray } from "./tray";
import { FocusedClient, Workspaces } from "./hyprland";
import { SingleMonitorProps } from "./utils";

export default function Bar(gdkmonitor: Gdk.Monitor) {
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
function Left({ gdkmonitor }: SingleMonitorProps): JSX.Element {
  return (
    <box spacing={20} className="workspaces">
      <Workspaces gdkmonitor={gdkmonitor} />
      <FocusedClient gdkmonitor={gdkmonitor} />
    </box>
  );
}

function Center(): JSX.Element {
  return (
    <box spacing={20} halign={Gtk.Align.START}>
      <Clock />
      <Weather />
      <Media />
    </box>
  );
}

function Right(): JSX.Element {
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
