import AstalNetwork from "gi://AstalNetwork";
import { createBinding, createComputed, createState, For, With } from "ags";
import { Gtk } from "ags/gtk4";
import { NETWORK_WIFI_ICONS } from "./constants";
import { getNetworkIcon, percentageToIconFromList } from "./utils";

export const WiFiMenu = ({ network }: { network: AstalNetwork.Network }) => {
  const connectivityBinding = createBinding(network, "connectivity");
  const primaryBinding = createBinding(network, "primary");
  const stateBinding = createBinding(network, "state");
  const wifiBinding = createBinding(network, "wifi");
  const accessPoints = createBinding(network.wifi, "accessPoints");
  const scanning = createBinding(network.wifi, "scanning");

  const [showPasswordDialog, setShowPasswordDialog] =
    createState<AstalNetwork.AccessPoint | null>(null);

  const handleConnect = async (accessPoint: AstalNetwork.AccessPoint) => {
    accessPoint.activate("", (source, res, data) => {
      const result = source?.activate_finish(res);
      console.log(result);
      data
        ? console.log(`connected to ${accessPoint.ssid}`)
        : console.error(`didn't connect to ${accessPoint.ssid}`);
    });
  };

  const handlePasswordConnect = async (password: string) => {
    const ap = showPasswordDialog.get();
    if (ap) {
      try {
        await ap.activate(password);
        setShowPasswordDialog(null);
      } catch (error) {
        console.error("Failed to connect with password:", error);
      }
    }
  };

  const handleScan = async () => {
    network.wifi?.scan();
  };

  const toggleWifi = async ({ active }: Gtk.Switch) => {
    network.wifi?.set_enabled(active);
  };

  const icon = createComputed(
    [connectivityBinding, primaryBinding, stateBinding, wifiBinding],
    getNetworkIcon,
  );

  return (
    <box orientation={Gtk.Orientation.VERTICAL} spacing={16} vexpand>
      <box
        orientation={Gtk.Orientation.HORIZONTAL}
        spacing={20}
        hexpand
        class="content-title"
      >
        <label label={icon} />
        <label label="WiFi" halign={Gtk.Align.START} hexpand />
        <switch
          active={network.wifi.enabled}
          onNotifyActive={toggleWifi}
          halign={Gtk.Align.END}
          valign={Gtk.Align.END}
        />
      </box>
      <scrolledwindow
        vscrollbarPolicy={Gtk.PolicyType.AUTOMATIC}
        hscrollbarPolicy={Gtk.PolicyType.NEVER}
        vexpand
      >
        <box orientation={Gtk.Orientation.VERTICAL} spacing={16}>
          <box orientation={Gtk.Orientation.VERTICAL} spacing={4}>
            <label
              halign={Gtk.Align.START}
              visible={wifiBinding.as((w) => w.ssid).as(Boolean)}
              label={wifiBinding.as((w) => `Connected to ${w.ssid}`)}
            />
            <label
              halign={Gtk.Align.START}
              label={connectivityBinding.as((c) => {
                switch (c) {
                  case AstalNetwork.Connectivity.FULL:
                    return "Full connectivity";
                  case AstalNetwork.Connectivity.LIMITED:
                    return "Limited connectivity";
                  case AstalNetwork.Connectivity.NONE:
                    return "No connectivity";
                  case AstalNetwork.Connectivity.PORTAL:
                    return "Sign-in needed";
                  case AstalNetwork.Connectivity.UNKNOWN:
                    return "Connectivity unknown";
                }
              })}
            />
            <label
              halign={Gtk.Align.START}
              label={stateBinding.as((s) => {
                switch (s) {
                  case AstalNetwork.State.ASLEEP:
                    return "Sleeping";
                  case AstalNetwork.State.CONNECTED_GLOBAL:
                    return "Global access";
                  case AstalNetwork.State.CONNECTED_LOCAL:
                    return "Local access only";
                  case AstalNetwork.State.CONNECTED_SITE:
                    return "Site access only";
                  case AstalNetwork.State.CONNECTING:
                    return "Connecting";
                  case AstalNetwork.State.DISCONNECTED:
                    return "Disconnected";
                  case AstalNetwork.State.DISCONNECTING:
                    return "Disconnecting";
                  case AstalNetwork.State.UNKNOWN:
                    return "State unknown";
                }
              })}
            />
          </box>
          {showPasswordDialog.get() ? (
            <PasswordDialog
              accessPoint={showPasswordDialog.get()!}
              onConnect={handlePasswordConnect}
              onCancel={() => setShowPasswordDialog(null)}
            />
          ) : (
            <box orientation={Gtk.Orientation.VERTICAL} spacing={8} vexpand>
              <box hexpand>
                <label
                  label="Available networks"
                  class="bold"
                  halign={Gtk.Align.START}
                  hexpand
                />
                <button
                  onClicked={handleScan}
                  sensitive={scanning.as((s) => !s)}
                  halign={Gtk.Align.END}
                >
                  <With value={scanning}>
                    {(s) =>
                      s
                        ? ((<Gtk.Spinner spinning />) as Gtk.Widget)
                        : ((<image iconName="view-refresh" />) as Gtk.Widget)
                    }
                  </With>
                </button>
              </box>

              <box orientation={Gtk.Orientation.VERTICAL} spacing={4}>
                <For
                  each={accessPoints.as((ap) =>
                    ap
                      .filter((p) => Boolean(p.ssid))
                      .sort((a, b) => b.strength - a.strength),
                  )}
                >
                  {(ap: AstalNetwork.AccessPoint) => (
                    <WiFiNetworkItem
                      accessPoint={ap}
                      onConnect={handleConnect}
                    />
                  )}
                </For>
              </box>
            </box>
          )}
        </box>
      </scrolledwindow>
    </box>
  );
};

const PasswordDialog = ({
  accessPoint,
  onConnect,
  onCancel,
}: {
  accessPoint: AstalNetwork.AccessPoint;
  onConnect: (password: string) => void;
  onCancel: () => void;
}) => {
  const [password, setPassword] = createState("");

  return (
    <box orientation={Gtk.Orientation.VERTICAL} spacing={8}>
      <label label={`Enter password for ${accessPoint.get_ssid()}`} />
      <entry visibility={false} placeholderText="Password" />
      <box spacing={8}>
        <button label="Cancel" onClicked={onCancel} />
        <button
          label="Connect"
          onClicked={() => onConnect("password")} // Simplified for now
        />
      </box>
    </box>
  );
};

const WiFiNetworkItem = ({
  accessPoint,
  onConnect,
}: {
  accessPoint: AstalNetwork.AccessPoint;
  onConnect: (ap: AstalNetwork.AccessPoint) => void;
}) => {
  const strength = createBinding(accessPoint, "strength");
  const ssid = createBinding(accessPoint, "ssid").as(String);
  const requiresPassword = createBinding(accessPoint, "requiresPassword");

  const strengthIcon = createComputed(
    [strength, requiresPassword],
    (strength, requiresPassword) =>
      requiresPassword
        ? percentageToIconFromList(strength / 100, NETWORK_WIFI_ICONS.vpn)
        : percentageToIconFromList(
            strength / 100,
            NETWORK_WIFI_ICONS.connected,
          ),
  );

  return (
    <button onClicked={() => onConnect(accessPoint)}>
      <box spacing={8} halign={Gtk.Align.START} hexpand>
        <label label={strengthIcon} widthRequest={32} />
        <label label={ssid} />
        <label
          class="dim access-point-frequency"
          label={`${(accessPoint.frequency / 1000).toFixed(1)} GHz`}
          hexpand
          halign={Gtk.Align.START}
        />
      </box>
    </button>
  );
};
