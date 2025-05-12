import Hyprland from "gi://AstalHyprland";
import { bind } from "astal";

import { type SingleMonitorProps, trunc } from "./utils.tsx";

export function Workspaces({ gdkmonitor }: SingleMonitorProps) {
  const hypr = Hyprland.get_default();

  return (
    <box cssClasses={["workspaces"]}>
      {bind(hypr, "workspaces").as((wss) =>
        wss
          .filter((ws) => ws.id > 0 && ws.monitor.name === gdkmonitor.connector)
          .sort((a, b) => a.id - b.id)
          .map((ws) => (
            <button
              cssClasses={bind(hypr, "focusedWorkspace").as((fw) => [
                ws === fw ? "bright" : "dim",
              ])}
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

  return (
    <box visible={focused.as((f) => f?.monitor?.name === gdkmonitor.connector)}>
      {focused.as(
        (client) =>
          client && (
            <label
              cssClasses={["dim"]}
              label={bind(client, "title").as((s) => trunc(s || ""))}
            />
          ),
      )}
    </box>
  );
}
