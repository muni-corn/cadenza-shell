import AstalNetwork from "gi://AstalNetwork";
import { createBinding, createComputed } from "ags";
import {
  Attention,
  percentageToIconFromList,
  Tile,
  unreachable,
} from "../utils";
import { NETWORK_WIFI_ICONS, NETWORK_WIRED_ICONS } from "../constants";

function getIcon(
  connectivity: AstalNetwork.Connectivity,
  primary: AstalNetwork.Primary,
  state: AstalNetwork.State,
  wifi: AstalNetwork.Wifi,
): string {
  if (primary === AstalNetwork.Primary.UNKNOWN) {
    return NETWORK_WIRED_ICONS.disabled;
  }

  const wifiConnectedIcon = percentageToIconFromList(
    wifi?.strength || 0 / 100,
    NETWORK_WIFI_ICONS.connected,
  );
  const wifiConnectedPacketLossIcon = percentageToIconFromList(
    wifi?.strength || 0 / 100,
    NETWORK_WIFI_ICONS.packetLoss,
  );

  switch (state) {
    case AstalNetwork.State.ASLEEP:
      return primary === AstalNetwork.Primary.WIRED
        ? NETWORK_WIRED_ICONS.disabled
        : NETWORK_WIFI_ICONS.disabled;
    case AstalNetwork.State.CONNECTING:
    case AstalNetwork.State.DISCONNECTED:
    case AstalNetwork.State.DISCONNECTING:
      return primary === AstalNetwork.Primary.WIRED
        ? NETWORK_WIRED_ICONS.disabled
        : NETWORK_WIFI_ICONS.disconnected;
    case AstalNetwork.State.CONNECTED_GLOBAL:
    case AstalNetwork.State.CONNECTED_LOCAL:
    case AstalNetwork.State.CONNECTED_SITE: {
      if (primary === AstalNetwork.Primary.WIRED) {
        return NETWORK_WIRED_ICONS.connected;
      }
      switch (connectivity) {
        case AstalNetwork.Connectivity.FULL:
          return wifiConnectedIcon;
        case AstalNetwork.Connectivity.LIMITED:
        case AstalNetwork.Connectivity.PORTAL:
        case AstalNetwork.Connectivity.UNKNOWN:
          return wifiConnectedPacketLossIcon;
        case AstalNetwork.Connectivity.NONE:
          return NETWORK_WIFI_ICONS.disconnected;
        default:
          return NETWORK_WIFI_ICONS.unknown;
      }
    }
    case AstalNetwork.State.UNKNOWN:
      return primary === AstalNetwork.Primary.WIRED
        ? NETWORK_WIRED_ICONS.unknown
        : NETWORK_WIFI_ICONS.unknown;
    default:
      unreachable(state);
  }
}

function getStatusText(
  primary: AstalNetwork.Primary,
  state: AstalNetwork.State,
  connectivity: AstalNetwork.Connectivity,
): string {
  if (
    state === AstalNetwork.State.ASLEEP ||
    primary === AstalNetwork.Primary.UNKNOWN
  ) {
    return "";
  }

  switch (state) {
    case AstalNetwork.State.CONNECTING:
      return "Connecting";
    case AstalNetwork.State.DISCONNECTED:
      return "Not connected";
    case AstalNetwork.State.DISCONNECTING:
      return "Disconnecting";
    case AstalNetwork.State.CONNECTED_GLOBAL:
    case AstalNetwork.State.CONNECTED_LOCAL:
    case AstalNetwork.State.CONNECTED_SITE: {
      if (primary === AstalNetwork.Primary.WIRED) {
        return "Connected";
      }
      switch (connectivity) {
        case AstalNetwork.Connectivity.FULL:
          return "";
        case AstalNetwork.Connectivity.LIMITED:
          return "Limited";
        case AstalNetwork.Connectivity.PORTAL:
          return "Sign-in needed";
        case AstalNetwork.Connectivity.NONE:
          return "No connectivity";
        default:
          return "Connectivity unknown";
      }
    }
    case AstalNetwork.State.UNKNOWN:
      return "State unknown";
    default:
      unreachable(state);
  }
}

export const Network = () => {
  const network = AstalNetwork.get_default();

  const connectivityBinding = createBinding(network, "connectivity");
  const primaryBinding = createBinding(network, "primary");
  const stateBinding = createBinding(network, "state");
  const wifiBinding = createBinding(network, "wifi");

  const icon = createComputed(
    [connectivityBinding, primaryBinding, stateBinding, wifiBinding],
    getIcon,
  );

  const primary = createComputed(
    [primaryBinding, wifiBinding],
    (primary, wifi) =>
      primary === AstalNetwork.Primary.WIFI ? wifi?.get_ssid() : "",
  );

  const secondary = createComputed(
    [primaryBinding, stateBinding, connectivityBinding],
    getStatusText,
  );

  const attention = createComputed(
    [primaryBinding, stateBinding],
    (primary, state) =>
      state === AstalNetwork.State.ASLEEP ||
      primary === AstalNetwork.Primary.UNKNOWN
        ? Attention.Dim
        : Attention.Normal,
  );

  return (
    <Tile
      icon={icon}
      primary={primary}
      secondary={secondary}
      attention={attention}
    />
  );
};
