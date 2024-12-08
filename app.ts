import { App } from "astal/gtk3";
import style from "./style.scss";
import Bar from "./widget/bar";
import NotificationPopups, {
  NotificationMap,
} from "./widget/notifications/notifications";

App.start({
  css: style,
  instanceName: "muse-shell",
  main(...args: string[]) {
    print("args", args);
    App.get_monitors().map(Bar);

    // put notifications on the top-left-most monitor
    NotificationPopups(
      App.get_monitors().sort((a, b) => {
        const dx = a.get_geometry().x - b.get_geometry().x;
        if (!dx) {
          return a.get_geometry().y - b.get_geometry().y;
        } else {
          return dx;
        }
      })[0],
    );
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
