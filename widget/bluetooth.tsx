import { Tile, makeTile } from "./utils";
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

  const tile = Variable.derive(
    [bind(bluetooth, "adapters"), bind(bluetooth, "devices")],
    (adapters, devices): Tile => {
      if (!adapters.some((a) => a.powered)) {
        return {
          icon: "\u{F00B2}",
          primary: "",
          secondary: "",
          visible: true,
        };
      } else {
        const connectedDevices = devices.filter((d) => d.connected);
        switch (connectedDevices.length) {
          case 0:
            return {
              icon: "\u{F00AF}",
              primary: "",
              secondary: "",
              visible: true,
            };
          case 1:
            return {
              icon: "\u{F00B1}",
              primary: connectedDevices[0].name,
              secondary: "",
              visible: true,
            };
          default:
            return {
              icon: "\u{F00B1}",
              primary: "",
              secondary: `${connectedDevices.length} devices`,
              visible: true,
            };
        }
      }
    },
  );

  return makeTile(tile());
}
