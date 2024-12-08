import { Astal, Gtk, Gdk } from "astal/gtk3";
import Notifd from "gi://AstalNotifd";
import { type Subscribable } from "astal/binding";
import { Variable, bind, timeout } from "astal";
import NotificationCard from "./notification-card";

// see comment below in constructor
const TIMEOUT_DELAY = 10000;

// The purpose if this class is to replace Variable<Array<Widget>>
// with a Map<number, Widget> type in order to track notification widgets
// by their id, while making it conviniently bindable as an array
export class NotificationMap implements Subscribable {
  private static instance: NotificationMap;
  static get_default() {
    if (!this.instance) {
      this.instance = new NotificationMap();
    }

    return this.instance;
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
    let id = [...this.map.keys()].reverse()[0];
    if (id != null) {
      let notification = Notifd.get_default().get_notification(id);
      let action = notification.actions[0];
      if (action) {
        notification.invoke(action.id);
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
          notification: notifd.get_notification(id)!,

          // once hovering over the notification is done
          // destroy the widget without calling notification.dismiss()
          // so that it acts as a "popup" and we can still display it
          // in a notification center like widget
          // but clicking on the close button will close it
          onHoverLost: () => {},
          // onHoverLost: () => this.delete(id),

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
      gdkmonitor={gdkmonitor}
      exclusivity={Astal.Exclusivity.EXCLUSIVE}
      anchor={TOP | RIGHT}
      widthRequest={432}
    >
      <box vertical>{bind(notifs)}</box>
    </window>
  );
}
