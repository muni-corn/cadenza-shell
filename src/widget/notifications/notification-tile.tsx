import AstalNotifd from "gi://AstalNotifd";
import { createState, onCleanup } from "ags";
import { Attention, type SingleMonitorProps, Tile } from "../utils";
import {
  notificationCenterMonitor,
  notificationCenterVisible,
  setNotificationCenterMonitor,
  setNotificationCenterVisible,
} from "./notification-center";

const NOTIFICATIONS_NONE = "\udb80\udc9c";
const NOTIFICATIONS_NEW = "\udb84\udd6b";

export function NotificationTile({ gdkmonitor }: SingleMonitorProps) {
  const notifd = AstalNotifd.get_default();
  const [unreadCount, setUnreadCount] = createState(0);

  const updateCount = () => {
    const persistedNotifications = notifd
      .get_notifications()
      .filter((n) => !n.transient);
    setUnreadCount(persistedNotifications.length);
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
      <Tile
        icon={unreadCount.as((count) =>
          count ? NOTIFICATIONS_NEW : NOTIFICATIONS_NONE,
        )}
        primary={unreadCount.as((count) => (count > 0 ? count.toString() : ""))}
        attention={unreadCount.as((count) =>
          count ? Attention.Normal : Attention.Dim,
        )}
      />
    </button>
  );
}
