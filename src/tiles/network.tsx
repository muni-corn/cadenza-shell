import AstalNetwork from "gi://AstalNetwork";
import { createBinding, createComputed } from "ags";
import type { Gtk } from "ags/gtk4";
import { Attention, getNetworkIcon, Tile } from "../utils";

export const Network = () => {
  const network = AstalNetwork.get_default();

  const connectivityBinding = createBinding(network, "connectivity");
  const primaryBinding = createBinding(network, "primary");
  const stateBinding = createBinding(network, "state");
  const wifiBinding = createBinding(network, "wifi");

  const icon = createComputed(
    [connectivityBinding, primaryBinding, stateBinding, wifiBinding],
    getNetworkIcon,
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
