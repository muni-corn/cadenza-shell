import { App } from "astal/gtk4";
import style from "./style.scss";
import { Bar } from "./widget/bar.tsx";
import {
  NotificationMap,
  NotificationPopups,
} from "./widget/notifications/notifications.tsx";

App.start({
  css: style,
  instanceName: "muse-shell",
  main() {
    const monitors = App.get_monitors();

    // show bar on all monitors
    monitors.map(Bar);

    // show notifications on last monitor
    NotificationPopups(monitors[monitors.length - 1]);
  },

  // this runs in the main instance
  requestHandler(request: string, res: (response: unknown) => void) {
    if (request === "noti-act") {
      NotificationMap.get_default().activateTopNotification();
      res("done");
    }
  },

  client(message: (msg: string) => string, ...args: string[]): void {
    if (args[0] === "noti" && args[1] === "act") {
      message("noti-act");
    }
  },
});
