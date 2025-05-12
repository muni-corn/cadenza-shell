import Tray from "gi://AstalTray";
import { bind } from "astal";
import { Gtk } from "astal/gtk4";

export function SysTray() {
  const tray = Tray.get_default();

  return (
    <box>
      {bind(tray, "items").as((items) =>
        items.map((item) => {
          const popover = Gtk.PopoverMenu.new_from_model(item.menu_model);
          popover.insert_action_group("dbusmenu", item.actionGroup);
          return (
            <menubutton
              tooltip-text={bind(item, "tooltipMarkup")}
              popover={popover}
              halign={Gtk.Align.END}
            >
              <image gicon={bind(item, "gicon")} />
            </menubutton>
          );
        }),
      )}
    </box>
  );
}
