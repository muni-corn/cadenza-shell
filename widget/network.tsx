import { bind, Variable } from "astal";
import { makeTile, percentageToIconFromList, Tile, unreachable } from "./utils";

import AstalNetwork from "gi://AstalNetwork";

const WIFI_ICONS = {
  connected: ["\u{F092F}", "\u{F091F}", "\u{F0922}", "\u{F0925}", "\u{F0928}"],
  packetLoss: ["\u{F092B}", "\u{F0920}", "\u{F0923}", "\u{F0926}", "\u{F0929}"],
  vpn: ["\u{F092C}", "\u{F0921}", "\u{F0924}", "\u{F0927}", "\u{F092A}"],
  disconnected: "\u{F092F}",
  disabled: "\u{F092E}",
  unknown: "\u{F092B}",
};
function getWifiIcon({ state, internet, strength }: AstalNetwork.Wifi): string {
  switch (state) {
    case AstalNetwork.DeviceState.ACTIVATED:
      if (internet === AstalNetwork.Internet.DISCONNECTED) {
        return percentageToIconFromList(strength / 100, WIFI_ICONS.packetLoss);
        // } else if (network.vpn.activated_connections.length > 0) {
        //   return percentageToIconFromList(wifi.strength, WIFI_ICONS.vpn);
      } else {
        return percentageToIconFromList(strength / 100, WIFI_ICONS.connected);
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
function getWiredIcon({ state, internet }: AstalNetwork.Wired): string {
  switch (state) {
    case AstalNetwork.DeviceState.ACTIVATED:
      if (internet === AstalNetwork.Internet.DISCONNECTED) {
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

function transformState(state: AstalNetwork.DeviceState): string {
  switch (state) {
    case AstalNetwork.DeviceState.ACTIVATED:
      return "";
    case AstalNetwork.DeviceState.CONFIG:
      return "Configuring";
    case AstalNetwork.DeviceState.DEACTIVATING:
      return "Deactivating";
    case AstalNetwork.DeviceState.DISCONNECTED:
      return "Disconnected";
    case AstalNetwork.DeviceState.FAILED:
      return "Failed";
    case AstalNetwork.DeviceState.IP_CHECK:
      return "Checking IP";
    case AstalNetwork.DeviceState.IP_CONFIG:
      return "Configuring IP";
    case AstalNetwork.DeviceState.NEED_AUTH:
      return "Sign-in needed";
    case AstalNetwork.DeviceState.PREPARE:
      return "Preparing";
    case AstalNetwork.DeviceState.SECONDARIES:
      return "Waiting for secondaries";
    case AstalNetwork.DeviceState.UNAVAILABLE:
      return "Unavailable";
    case AstalNetwork.DeviceState.UNKNOWN:
      return "Unknown";
    case AstalNetwork.DeviceState.UNMANAGED:
      return "Unmanaged";
    default:
      return unreachable(state);
  }
}

export function Network() {
  const network = AstalNetwork.get_default();
  const wifi = network.wifi;
  const wired = network.wired;

  let wifiVar: Variable<AstalNetwork.Wifi | null> = Variable(null);
  if (wifi)
    wifiVar = Variable.derive(
      [
        bind(wifi, "internet"),
        bind(wifi, "ssid"),
        bind(wifi, "state"),
        bind(wifi, "strength"),
      ],
      () => network.get_wifi(),
    );

  let wiredVar: Variable<AstalNetwork.Wired | null> = Variable(null);
  if (wired)
    wiredVar = Variable.derive(
      [bind(wired, "internet"), bind(wired, "state")],
      () => network.get_wired(),
    );

  let tile: Variable<Tile> = Variable.derive(
    [bind(network, "primary"), wifiVar, wiredVar],
    (primary, wifi, wired) => {
      const icon =
        primary === AstalNetwork.Primary.WIRED && wired
          ? getWiredIcon(wired)
          : wifi
            ? getWifiIcon(wifi)
            : "";

      const secondary = transformState(
        primary === AstalNetwork.Primary.WIRED && wired
          ? wired.get_state()
          : wifi
            ? wifi.get_state()
            : -999,
      );
      return {
        icon,
        primary:
          primary === AstalNetwork.Primary.WIRED ? "" : wifi?.get_ssid() || "",
        secondary,
      };
    },
  );

  return makeTile(tile());
}
