import { bind, Variable } from "astal";
import { makeTile, percentageToIconFromList, Tile } from "./utils";

import AstalNetwork from "gi://AstalNetwork";

const network = AstalNetwork.get_default();

const WIFI_ICONS = {
  connected: ["\u{F092F}", "\u{F091F}", "\u{F0922}", "\u{F0925}", "\u{F0928}"],
  packetLoss: ["\u{F092B}", "\u{F0920}", "\u{F0923}", "\u{F0926}", "\u{F0929}"],
  vpn: ["\u{F092C}", "\u{F0921}", "\u{F0924}", "\u{F0927}", "\u{F092A}"],
  disconnected: "\u{F092F}",
  disabled: "\u{F092E}",
  unknown: "\u{F092B}",
};
function getWifiIcon(): string {
  const wifi = network.wifi;

  switch (wifi.state) {
    case AstalNetwork.DeviceState.ACTIVATED:
      if (wifi.internet === AstalNetwork.Internet.DISCONNECTED) {
        return percentageToIconFromList(
          wifi.strength / 100,
          WIFI_ICONS.packetLoss,
        );
        // } else if (network.vpn.activated_connections.length > 0) {
        //   return percentageToIconFromList(wifi.strength, WIFI_ICONS.vpn);
      } else {
        return percentageToIconFromList(
          wifi.strength / 100,
          WIFI_ICONS.connected,
        );
      }
    case AstalNetwork.DeviceState.UNAVAILABLE:
      return WIFI_ICONS.disabled;
    case AstalNetwork.DeviceState.DISCONNECTED:
      return WIFI_ICONS.disconnected;
    default:
      return WIFI_ICONS.unknown;
  }
}

const WIRED_ICONS = {
  connected: "\u{F059F}",
  packetLoss: "\u{F0551}",
  vpn: "\u{F0582}",
  disabled: "\u{F0A8E}",
  unknown: "\u{F0A39}",
};
function getWiredIcon(): string {
  const wired = network.wired;

  switch (wired.state) {
    case AstalNetwork.DeviceState.ACTIVATED:
      if (wired.internet === AstalNetwork.Internet.DISCONNECTED) {
        return WIRED_ICONS.packetLoss;
        // } else if (network.vpn.activated_connections.length > 0) {
        //   return WIRED_ICONS.vpn;
      } else {
        return WIRED_ICONS.connected;
      }
    case AstalNetwork.DeviceState.UNAVAILABLE:
    case AstalNetwork.DeviceState.DISCONNECTED:
      return WIRED_ICONS.disabled;
    default:
      return WIRED_ICONS.unknown;
  }
}

function transformState(state: AstalNetwork.Wifi["state"]): string {
  switch (state) {
    case AstalNetwork.DeviceState.ACTIVATED:
      return "";
    case AstalNetwork.DeviceState.NEED_AUTH:
      return "Sign-in needed";
    case AstalNetwork.DeviceState.CONFIG:
      return "Configuring";
    case AstalNetwork.DeviceState.PREPARE:
      return "Preparing";
    case AstalNetwork.DeviceState.SECONDARIES:
      return "Waiting for secondaries";
    case AstalNetwork.DeviceState.IP_CHECK:
      return "Checking IP";
    case AstalNetwork.DeviceState.IP_CONFIG:
      return "Configuring IP";
    default:
      let transformed = state.toString();
      transformed = transformed.replace("_", " ");
      transformed = transformed.replace("ip", "IP");
      transformed = transformed.charAt(0).toUpperCase() + transformed.slice(1);

      return transformed;
  }
}

export function Network() {
  let tile: Variable<Tile> = Variable.derive(
    [bind(network, "primary"), bind(network, "wifi"), bind(network, "wired")],
    (primary, wifi, wired) => {
      return {
        icon:
          primary === AstalNetwork.Primary.WIRED
            ? getWiredIcon()
            : getWifiIcon(),
        primary: primary === AstalNetwork.Primary.WIRED ? "" : wifi.ssid || "",
        secondary: transformState(
          primary === AstalNetwork.Primary.WIRED ? wired.state : wifi.state,
        ),
      };
    },
  );

  return makeTile(tile());
}
