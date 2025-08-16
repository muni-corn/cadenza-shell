import AstalBluetooth from "gi://AstalBluetooth";
import { createBinding, createComputed } from "ags";

const _BLUETOOTH_BATTERY_ICONS = [
  "\u{F093E}",
  "\u{F093F}",
  "\u{F0940}",
  "\u{F0941}",
  "\u{F0942}",
  "\u{F0943}",
  "\u{F0944}",
  "\u{F0945}",
  "\u{F0946}",
  "\u{F0948}",
];
const _BLUETOOTH_BATTERY_UNKNOWN_ICON = "\u{F094A}";

export const Bluetooth = () => {
  const bluetooth = AstalBluetooth.Bluetooth.get_default();

  if (bluetooth?.adapter) {
    const powered = createBinding(bluetooth, "is_powered");
    const icon = createComputed(
      [powered, createBinding(bluetooth, "is_connected")],
      (powered, connected) => {
        if (!powered) {
          return "\u{F00B2}";
        }
        if (connected) {
          return "\u{F00B1}";
        }
        return "\u{F00AF}";
      },
    );

    return (
      <label
        label={icon}
        class={powered.as((p) => (p ? "icon tile" : "icon dim tile"))}
        widthRequest={16}
      />
    );
  }

  return null;
};
