import Hyprland from "gi://AstalHyprland";
import { createBinding, For, With } from "ags";

import { type SingleMonitorProps, trunc } from "./utils";

export const Workspaces = ({ gdkmonitor }: SingleMonitorProps) => {
  const hypr = Hyprland.get_default();
  const workspaces = createBinding(hypr, "workspaces").as((wss) =>
    wss
      .filter((ws) => ws.id > 0 && ws.monitor.name === gdkmonitor.connector)
      .sort((a, b) => a.id - b.id),
  );

  return (
    hypr && (
      <box class="workspaces">
        <For each={workspaces}>
          {(ws) => {
            const activeClass = createBinding(hypr, "focusedWorkspace").as(
              (fw) => (ws === fw ? "bright" : "dim"),
            );

            return (
              <button class={activeClass} onClicked={() => ws.focus()}>
                {ws.id}
              </button>
            );
          }}
        </For>
      </box>
    )
  );
};

export const FocusedClient = ({ gdkmonitor }: SingleMonitorProps) => {
  const hypr = Hyprland.get_default();
  if (!hypr) {
    return <></>;
  }

  const focused = createBinding(hypr, "focusedClient");

  return (
    <box visible={focused.as((f) => f?.monitor?.name === gdkmonitor.connector)}>
      <With value={focused}>
        {(client) =>
          client && (
            <label
              class="dim"
              label={createBinding(client, "title").as((s) => trunc(s || ""))}
            />
          )
        }
      </With>
    </box>
  );
};
