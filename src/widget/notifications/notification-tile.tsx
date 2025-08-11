import AstalNotifd from "gi://AstalNotifd";
import { createState, onCleanup } from "ags";
import type { SingleMonitorProps } from "../utils";
import {
  notificationCenterMonitor,
  notificationCenterVisible,
  setNotificationCenterMonitor,
  setNotificationCenterVisible,
} from "./notification-center";

export function NotificationTile({ gdkmonitor }: SingleMonitorProps) {
  const notifd = AstalNotifd.get_default();
  const [unreadCount, setUnreadCount] = createState(0);

  const updateCount = () => {
    const notifications = notifd.get_notifications();
    setUnreadCount(notifications.length);
  };

  const notifiedHandler = notifd.connect("notified", updateCount);
  const resolvedHandler = notifd.connect("resolved", updateCount);

  updateCount();

  onCleanup(() => {
    notifd.disconnect(notifiedHandler);
    notifd.disconnect(resolvedHandler);
  });

  const toggleNotificationCenter = () => {
    setNotificationCenterVisible(
      gdkmonitor !== notificationCenterMonitor.get() ||
        !notificationCenterVisible.get(),
    );
    setNotificationCenterMonitor(gdkmonitor);
  };

  return (
    <button class="notification-tile" onClicked={toggleNotificationCenter}>
      <box spacing={4}>
        <image iconName="notification-symbolic" />
        <label
          label={unreadCount((count) => (count > 0 ? count.toString() : ""))}
          visible={unreadCount((count) => count > 0)}
          class="notification-count"
        />
      </box>
    </button>
  );
}
