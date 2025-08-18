import AstalNetwork from "gi://AstalNetwork";
import { createBinding, createComputed } from "ags";
import { Gtk } from "ags/gtk4";
import { Attention, getNetworkIcon, Tile } from "../utils";
import { WiFiMenu } from "../wifi-menu";

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

  const attention = createComputed(
    [primaryBinding, stateBinding],
    (primary, state) =>
      state === AstalNetwork.State.ASLEEP ||
      primary === AstalNetwork.Primary.UNKNOWN
        ? Attention.Dim
        : Attention.Normal,
  );

  return (
    <menubutton class="bar-button">
      <Tile icon={icon} attention={attention} />
      <popover widthRequest={400} heightRequest={400}>
        <WiFiMenu network={network} />
      </popover>
    </menubutton>
  );
};
