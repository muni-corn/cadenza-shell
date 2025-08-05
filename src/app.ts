import app from "ags/gtk4/app";
import style from "./style.scss";
import { Bar } from "./widget/bar";
import { Notifications } from "./widget/notifications/notifications";

app.start({
  css: style,
  instanceName: "muse-shell",
  main() {
    const monitors = app.get_monitors();

    // show bar on all monitors
    monitors.map(Bar);

    Notifications({ gdkmonitor: monitors[0] });
  },

  // this runs in the main instance
  requestHandler(request: string, res: (response: unknown) => void) {
    if (request === "noti-act") {
      // TODO NotificationMap.get_default().activateTopNotification();
      res("done");
    }
  },

  client(message: (msg: string) => string, ...args: string[]): void {
    if (args[0] === "noti" && args[1] === "act") {
      message("noti-act");
    }
  },
});
