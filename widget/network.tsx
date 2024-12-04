import { bind, Variable } from "astal";
import { makeTile, percentageToIconFromList, Tile, unreachable } from "./utils";

import AstalNetwork from "gi://AstalNetwork";

const WIRED_ICONS = {
  connected: "\u{F059F}",
  packetLoss: "\u{F0551}",
  vpn: "\u{F0582}",
  disabled: "\u{F0A8E}",
  unknown: "\u{F0A39}",
};
const WIFI_ICONS = {
  connected: ["\u{F092F}", "\u{F091F}", "\u{F0922}", "\u{F0925}", "\u{F0928}"],
  packetLoss: ["\u{F092B}", "\u{F0920}", "\u{F0923}", "\u{F0926}", "\u{F0929}"],
  vpn: ["\u{F092C}", "\u{F0921}", "\u{F0924}", "\u{F0927}", "\u{F092A}"],
  disconnected: "\u{F092F}",
  disabled: "\u{F092E}",
  unknown: "\u{F092B}",
};
function getIcon({
  connectivity,
  primary,
  state,
  wifi: { strength },
}: AstalNetwork.Network): string {
  let wifiConnectedIcon = percentageToIconFromList(
    strength / 100,
    WIFI_ICONS.connected,
  );
  let wifiConnectedPacketLossIcon = percentageToIconFromList(
    strength / 100,
    WIFI_ICONS.packetLoss,
  );

  switch (state) {
    case AstalNetwork.State.ASLEEP:
      return primary === AstalNetwork.Primary.WIRED
        ? WIRED_ICONS.disabled
        : WIFI_ICONS.disabled;
    case AstalNetwork.State.CONNECTING:
    case AstalNetwork.State.DISCONNECTED:
    case AstalNetwork.State.DISCONNECTING:
      return primary === AstalNetwork.Primary.WIRED
        ? WIRED_ICONS.disabled
        : WIFI_ICONS.disconnected;
    case AstalNetwork.State.CONNECTED_GLOBAL:
    case AstalNetwork.State.CONNECTED_LOCAL:
    case AstalNetwork.State.CONNECTED_SITE:
      if (primary === AstalNetwork.Primary.WIRED) {
        return WIRED_ICONS.connected;
      } else {
        switch (connectivity) {
          case AstalNetwork.Connectivity.FULL:
            return wifiConnectedIcon;
          case AstalNetwork.Connectivity.LIMITED:
          case AstalNetwork.Connectivity.PORTAL:
          case AstalNetwork.Connectivity.UNKNOWN:
            return wifiConnectedPacketLossIcon;
          case AstalNetwork.Connectivity.NONE:
            return WIFI_ICONS.disconnected;
        }
      }
    case AstalNetwork.State.UNKNOWN:
      return primary === AstalNetwork.Primary.WIRED
        ? WIRED_ICONS.unknown
        : WIFI_ICONS.unknown;
    default:
      unreachable(state);
  }
}

function getStatusText({
  connectivity,
  primary,
  state,
}: AstalNetwork.Network): string {
  switch (state) {
    case AstalNetwork.State.ASLEEP:
      return "Off";
    case AstalNetwork.State.CONNECTING:
      return "Connecting";
    case AstalNetwork.State.DISCONNECTED:
      return "Not connected";
    case AstalNetwork.State.DISCONNECTING:
      return "Disconnecting";
    case AstalNetwork.State.CONNECTED_GLOBAL:
    case AstalNetwork.State.CONNECTED_LOCAL:
    case AstalNetwork.State.CONNECTED_SITE:
      if (primary === AstalNetwork.Primary.WIRED) {
        return "Connected";
      } else {
        switch (connectivity) {
          case AstalNetwork.Connectivity.FULL:
            return "";
          case AstalNetwork.Connectivity.LIMITED:
            return "Limited";
          case AstalNetwork.Connectivity.PORTAL:
            return "Sign-in needed";
          case AstalNetwork.Connectivity.UNKNOWN:
            return "Connectivity unknown";
          case AstalNetwork.Connectivity.NONE:
            return "No connectivity";
        }
      }
    case AstalNetwork.State.UNKNOWN:
      return "State unknown";
    default:
      unreachable(state);
  }
}

export function Network() {
  const network = AstalNetwork.get_default();

  let networkVar: Variable<AstalNetwork.Network> = Variable.derive(
    [
      bind(network, "primary"),
      bind(network, "connectivity"),
      bind(network, "state"),
    ],
    () => network,
  );

  let tile: Variable<Tile> = Variable.derive([networkVar], (network) => {
    const icon = getIcon(network);
    const secondary = getStatusText(network);
    return {
      icon,
      primary:
        network.primary === AstalNetwork.Primary.WIRED
          ? ""
          : network.get_wifi()?.get_ssid() || "",
      secondary,
    };
  });

  return makeTile(tile());
}
