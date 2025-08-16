import Tray from "gi://AstalTray";
import { createBinding, createState, For } from "ags";
import { Gtk } from "ags/gtk4";

export const SysTray = () => {
  const tray = Tray.get_default();

  const [expanded, setExpanded] = createState(false);

  const items = createBinding(tray, "items");

  return (
    <box spacing={0}>
      <revealer
        revealChild={expanded}
        transitionType={Gtk.RevealerTransitionType.SLIDE_LEFT}
      >
        <box>
          <For each={items}>
            {(item) => {
              const popover = Gtk.PopoverMenu.new_from_model(item.menu_model);
              popover.insert_action_group("dbusmenu", item.actionGroup);
              return (
                <menubutton
                  tooltip-text={createBinding(item, "tooltipMarkup")}
                  popover={popover}
                  halign={Gtk.Align.END}
                  class="bar-button"
                  widthRequest={32}
                >
                  <image
                    gicon={createBinding(item, "gicon")}
                    halign={Gtk.Align.CENTER}
                  />
                </menubutton>
              );
            }}
          </For>
        </box>
      </revealer>
      <button
        onClicked={() => setExpanded(!expanded.get())}
        iconName={expanded((e) => (e ? "arrow-right" : "arrow-left"))}
        class="bar-button"
      />
    </box>
  );
};
