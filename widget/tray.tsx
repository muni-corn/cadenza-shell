import Tray from "gi://AstalTray";
import { bind } from "astal";
import { Gtk } from "astal/gtk4";

export function SysTray() {
  const tray = Tray.get_default();

  return (
    <box>
      {bind(tray, "items").as((items) =>
        items.map((item) => (
          <menubutton
            tooltip-text={bind(item, "tooltipMarkup")}
            menuModel={bind(item, "menuModel").as((model) => model || null)}
            halign={Gtk.Align.END}
          >
            <image gicon={bind(item, "gicon")} />
          </menubutton>
        )),
      )}
    </box>
  );
}
