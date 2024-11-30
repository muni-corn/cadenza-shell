import Hyprland from "gi://AstalHyprland";
import { bind } from "astal";
import { SingleMonitorProps } from "./utils";
import { Gdk } from "astal/gtk3";

export function Workspaces({ gdkmonitor }: SingleMonitorProps) {
  const hypr = Hyprland.get_default();
  const monitorName = getMonitorName(gdkmonitor);

  return (
    <box className="Workspaces">
      {bind(hypr, "workspaces").as((wss) =>
        wss
          .filter((ws) => ws.id > 0 && ws.monitor.name === monitorName)
          .sort((a, b) => a.id - b.id)
          .map((ws) => (
            <button
              className={bind(hypr, "focusedWorkspace").as((fw) =>
                ws === fw ? "primary" : "dim",
              )}
              onClicked={() => ws.focus()}
            >
              {ws.id}
            </button>
          )),
      )}
    </box>
  );
}

export function FocusedClient({ gdkmonitor }: SingleMonitorProps) {
  const hypr = Hyprland.get_default();

  const focused = bind(hypr, "focusedClient");
  const monitorName = getMonitorName(gdkmonitor);

  return (
    <box visible={focused.as((f) => f.monitor.name === monitorName)}>
      {focused.as(
        (client) =>
          client && (
            <label className="dim" label={bind(client, "title").as(String)} />
          ),
      )}
    </box>
  );
}

const display = Gdk.Display.get_default();
function getMonitorName(gdkmonitor: Gdk.Monitor) {
  if (display) {
    const screen = display.get_default_screen();
    for (let i = 0; i < display.get_n_monitors(); ++i) {
      if (gdkmonitor === display.get_monitor(i))
        return screen.get_monitor_plug_name(i);
    }
  } else {
    return null;
  }
}
