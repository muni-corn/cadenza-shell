import { App } from "astal/gtk3";
import style from "./style.scss";
import Bar from "./widget/bar";
import NotificationPopups, {
  NotificationMap,
} from "./widget/notifications/notifications";

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
  requestHandler(request: string, res: (response: any) => void) {
    switch (request) {
      case "noti-act":
        NotificationMap.get_default().activateTopNotification();
        res("done");
        break;
    }
  },

  client(message: (msg: string) => string, ...args: string[]): void {
    if (args[0] === "noti" && args[1] === "act") message("noti-act");
  },
});
