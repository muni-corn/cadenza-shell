import AstalNotifd from "gi://AstalNotifd";
import { createState, For, onCleanup } from "ags";
import { Astal, Gtk } from "ags/gtk4";
import app from "ags/gtk4/app";
import { NotificationCard } from "./notification-card";

export function NotificationPopups() {
  const monitor = app.monitors[0];

  const notifd = AstalNotifd.get_default();

  const [notifications, setNotifications] = createState<
    AstalNotifd.Notification[]
  >([]);

  const notifiedHandler = notifd.connect("notified", (_, id, replaced) => {
    const notification = notifd.get_notification(id);

    if (replaced && notifications.get().some((n) => n.id === id)) {
      setNotifications((ns) => ns.map((n) => (n.id === id ? notification : n)));
    } else {
      setNotifications((ns) => [notification, ...ns]);
    }
  });

  const resolvedHandler = notifd.connect("resolved", (_, id) => {
    setNotifications((ns) => ns.filter((n) => n.id !== id));
  });

  // technically, we don't need to cleanup because in this example this is a root component
  // and this cleanup function is only called when the program exits, but exiting will cleanup either way
  // but it's here to remind you that you should not forget to cleanup signal connections
  onCleanup(() => {
    notifd.disconnect(notifiedHandler);
    notifd.disconnect(resolvedHandler);
  });

  return (
    <window
      visible={notifications((ns) => ns.length > 0)}
      class="notifications"
      namespace="notifications"
      gdkmonitor={monitor}
      exclusivity={Astal.Exclusivity.EXCLUSIVE}
      anchor={Astal.WindowAnchor.TOP | Astal.WindowAnchor.RIGHT}
      widthRequest={432}
    >
      <box orientation={Gtk.Orientation.VERTICAL}>
        <For each={notifications}>
          {(notification) => <NotificationCard notification={notification} />}
        </For>
      </box>
    </window>
  );
}
