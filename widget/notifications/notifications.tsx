import Notifd from "gi://AstalNotifd";
import { Variable, bind, timeout } from "astal";
import type { Subscribable } from "astal/binding";
import { Astal, type Gdk, type Gtk } from "astal/gtk3";
import NotificationCard from "./notification-card";

// see comment below in constructor
const TIMEOUT_DELAY = 10000;

// The purpose if this class is to replace Variable<Array<Widget>>
// with a Map<number, Widget> type in order to track notification widgets
// by their id, while making it conviniently bindable as an array
export class NotificationMap implements Subscribable {
  private static instance: NotificationMap;
  static get_default() {
    if (!NotificationMap.instance) {
      NotificationMap.instance = new NotificationMap();
    }

    return NotificationMap.instance;
  }

  // the underlying map to keep track of id widget pairs
  private map: Map<number, Gtk.Widget> = new Map();

  // it makes sense to use a Variable under the hood and use its
  // reactivity implementation instead of keeping track of subscribers ourselves
  private var: Variable<Array<Gtk.Widget>> = Variable([]);

  // notify subscribers to rerender when state changes
  private notify() {
    this.var.set([...this.map.values()].reverse());
  }

  activateTopNotification() {
    const id = [...this.map.keys()].reverse()[0];
    if (id != null) {
      const notification = Notifd.get_default().get_notification(id);
      const action = notification.actions[0];
      if (action != null) {
        notification.invoke(action.id);
        this.delete(id);
      }
    }
  }

  constructor() {
    const notifd = Notifd.get_default();

    /**
     * uncomment this if you want to
     * ignore timeout by senders and enforce our own timeout
     * note that if the notification has any actions
     * they might not work, since the sender already treats them as resolved
     */
    // notifd.ignoreTimeout = true

    notifd.connect("notified", (_, id) => {
      this.set(
        id,
        NotificationCard({
          notification: notifd.get_notification(id),

          // notifd by default does not close notifications
          // until user input or the timeout specified by sender
          // which we set to ignore above
          setup: () =>
            timeout(TIMEOUT_DELAY, () => {
              this.delete(id);
            }),

          onActionExecution: () => this.delete(id),
        }),
      );
    });

    // notifications can be closed by the outside before
    // any user input, which have to be handled too
    notifd.connect("resolved", (_, id) => {
      this.delete(id);
    });
  }

  private set(key: number, value: Gtk.Widget) {
    // in case of replacecment destroy previous widget
    this.map.get(key)?.destroy();
    this.map.set(key, value);
    this.notify();
  }

  private delete(key: number) {
    this.map.get(key)?.destroy();
    this.map.delete(key);
    this.notify();
  }

  // needed by the Subscribable interface
  get() {
    return this.var.get();
  }

  // needed by the Subscribable interface
  subscribe(callback: (list: Array<Gtk.Widget>) => void) {
    return this.var.subscribe(callback);
  }
}

export default function NotificationPopups(gdkmonitor: Gdk.Monitor) {
  const { TOP, RIGHT } = Astal.WindowAnchor;
  const notifs = new NotificationMap();

  return (
    <window
      className="notifications"
      namespace="notifications"
      gdkmonitor={gdkmonitor}
      exclusivity={Astal.Exclusivity.EXCLUSIVE}
      anchor={TOP | RIGHT}
      widthRequest={432}
    >
      <box vertical={true}>{bind(notifs)}</box>
    </window>
  );
}
