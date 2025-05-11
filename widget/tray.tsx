import Tray from "gi://AstalTray";
import { bind } from "astal";

export function SysTray() {
  const tray = Tray.get_default();

  return (
    <box>
      {bind(tray, "items").as((items) =>
        items.map((item) => (
          <menubutton
            tooltip-text={bind(item, "tooltipMarkup")}
            menuModel={bind(item, "menuModel").as((model) => model || null)}
          >
            <image icon-name={bind(item, "gicon").as(String)} />
          </menubutton>
        )),
      )}
    </box>
  );
}
