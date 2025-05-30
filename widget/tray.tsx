import Tray from "gi://AstalTray";
import { bind, Variable } from "astal";
import { Gtk } from "astal/gtk4";

export const SysTray = () => {
  const tray = Tray.get_default();

  const expanded = Variable(false);

  return (
    <box spacing={0}>
      <revealer
        revealChild={expanded()}
        transitionType={Gtk.RevealerTransitionType.SLIDE_LEFT}
      >
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
      </revealer>
      <button
        onClicked={() => expanded.set(!expanded.get())}
        iconName={expanded((e) => (e ? "arrow-right" : "arrow-left"))}
      />
    </box>
  );
};
