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
    let monitors = App.get_monitors();

    monitors.map(Bar);

    for (const m of App.get_monitors()) {
      const g = m.get_geometry();
      print(`${m.get_manufacturer()}: ${g.x},${g.y} ${g.width}x${g.height}`);
    }

    // show notifications on last monitor
    NotificationPopups(monitors[monitors.length - 1]);
  },

  // this runs in the main instance
  requestHandler(request: string, res: (response: any) => void) {
    print("in request handler");
    switch (request) {
      case "noti-act":
        print("executing noti-act");
        NotificationMap.get_default().activateTopNotification();
        res("done");
        break;
    }
  },

  client(message: (msg: string) => string, ...args: string[]): void {
    print("args:", args.join(" "));
    if (args[0] === "noti" && args[1] === "act") {
      print("sending noti-act message");
      let res = message("noti-act");
      print(res);
    } else {
      printerr("no commands with that name");
    }
  },
});
