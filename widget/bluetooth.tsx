import { bind, Variable } from "astal";
import AstalBluetooth from "gi://AstalBluetooth";

const BLUETOOTH_BATTERY_ICONS = [
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
const BLUETOOTH_BATTERY_UNKNOWN_ICON = "\u{F094A}";

export function Bluetooth() {
  const bluetooth = AstalBluetooth.Bluetooth.get_default();

  if (bluetooth?.adapter) {
    const powered = bind(bluetooth, "is_powered");
    const icon = Variable.derive(
      [powered, bind(bluetooth, "is_connected")],
      (powered, connected) => {
        if (!powered) {
          return "\u{F00B2}";
        } else if (connected) {
          return "\u{F00B1}";
        } else {
          return "\u{F00AF}";
        }
      },
    );

    return (
      <label
        label={icon()}
        className={powered.as((p) => (p ? "icon" : "icon dim"))}
        widthRequest={16}
      />
    );
  }

  return undefined;
}
