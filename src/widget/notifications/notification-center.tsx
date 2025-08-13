import AstalNotifd from "gi://AstalNotifd";
import { createState, For, onCleanup } from "ags";
import { Astal, Gtk } from "ags/gtk4";
import app from "ags/gtk4/app";
import { AnalogClock } from "../analog-clock";
import { NotificationCard } from "./notification-card";

export const [notificationCenterMonitor, setNotificationCenterMonitor] =
  createState(app.get_monitors()[0]);
export const [notificationCenterVisible, setNotificationCenterVisible] =
  createState(false);

export function NotificationCenter() {
  const notifd = AstalNotifd.get_default();

  const [newNotifications, setNewNotifications] = createState<
    AstalNotifd.Notification[]
  >(notifd.get_notifications());

  const notifiedHandler = notifd.connect("notified", (_, id, replaced) => {
    const notification = notifd.get_notification(id);

    if (replaced && newNotifications.get().some((n) => n.id === id)) {
      setNewNotifications((ns) =>
        ns.map((n) => (n.id === id ? notification : n)),
      );
    } else {
      setNewNotifications((ns) => [notification, ...ns]);
    }
  });

  const resolvedHandler = notifd.connect("resolved", (_, id) => {
    setNewNotifications((ns) => ns.filter((n) => n.id !== id));
  });

  onCleanup(() => {
    notifd.disconnect(notifiedHandler);
    notifd.disconnect(resolvedHandler);
  });

  const dismissAll = () => {
    newNotifications.get().forEach((n) => n.dismiss());
  };

  return (
    <window
      visible={notificationCenterVisible}
      class="notification-center"
      namespace="notification-center"
      gdkmonitor={notificationCenterMonitor}
      exclusivity={Astal.Exclusivity.NORMAL}
      anchor={
        Astal.WindowAnchor.TOP |
        Astal.WindowAnchor.RIGHT |
        Astal.WindowAnchor.BOTTOM
      }
      margin={8}
      widthRequest={432}
      keymode={Astal.Keymode.ON_DEMAND}
    >
      <box orientation={Gtk.Orientation.VERTICAL}>
        <box
          class="notification-center-header"
          orientation={Gtk.Orientation.HORIZONTAL}
          spacing={16}
        >
          <AnalogClock />
          <Gtk.Calendar />
        </box>

        <scrolledwindow vexpand={true} hscrollbarPolicy={Gtk.PolicyType.NEVER}>
          <box orientation={Gtk.Orientation.VERTICAL} spacing={8}>
            <label
              class="content-title"
              label="Notifications"
              hexpand={true}
              halign={Gtk.Align.START}
            />
            <button
              class="clear-all"
              onClicked={dismissAll}
              halign={Gtk.Align.END}
              visible={newNotifications((ns) => ns.length > 0)}
              vexpand={false}
            >
              <label label="Clear all" />
            </button>

            <box orientation={Gtk.Orientation.VERTICAL} spacing={4}>
              <For each={newNotifications}>
                {(notification) => (
                  <NotificationCard
                    notification={notification}
                    afterActionExecution={() => {}}
                  />
                )}
              </For>
              <box
                class="empty-state"
                visible={newNotifications((ns) => ns.length === 0)}
                orientation={Gtk.Orientation.VERTICAL}
                spacing={8}
                valign={Gtk.Align.CENTER}
              >
                <image iconName="notification-symbolic" class="empty-icon" />
                <label class="empty-text" label="No new notifications" />
              </box>
            </box>
          </box>
        </scrolledwindow>
      </box>
    </window>
  );
}
