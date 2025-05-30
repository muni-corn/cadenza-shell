import Notifd from "gi://AstalNotifd";
import { GLib } from "astal";
import { Gtk } from "astal/gtk4";

// TODO
// const isIcon = (icon: string) => !!Astal3.Icon.lookup_icon(icon);
const fileExists = (path: string) => GLib.file_test(path, GLib.FileTest.EXISTS);

const time = (time: number, format = "%-I:%M %P") =>
  GLib.DateTime.new_from_unix_local(time).format(format)!;

const urgency = (n: Notifd.Notification) => {
  const { LOW, CRITICAL } = Notifd.Urgency;
  // match operator when?
  switch (n.urgency) {
    case LOW:
      return "low";
    case CRITICAL:
      return "critical";
    default:
      return "normal";
  }
};

type Props = {
  setup(self: Gtk.Widget): void;
  onActionExecution(): void;
  notification: Notifd.Notification;
};

export const NotificationCard = (props: Props) => {
  const { notification: n, setup, onActionExecution } = props;
  const { START, CENTER, END } = Gtk.Align;

  const content = (
    <box cssClasses={["content"]}>
      {n.image && fileExists(n.image) ? (
        <image valign={START} cssClasses={["image"]} file={n.image} />
      ) : n.image ? (
        <box hexpand={false} valign={START} cssClasses={["icon-image"]}>
          <image
            iconName={n.image}
            hexpand={true}
            vexpand={true}
            halign={CENTER}
            valign={CENTER}
          />
        </box>
      ) : undefined}
      <box vertical={true}>
        <label
          cssClasses={["summary"]}
          wrap={true}
          halign={START}
          xalign={0}
          label={n.summary}
          lines={2}
        />
        {n.body && (
          <label
            cssClasses={["body"]}
            wrap={true}
            useMarkup={true}
            halign={START}
            xalign={0}
            label={n.body}
            lines={6}
          />
        )}
      </box>
    </box>
  );

  // wrap content in a button if it is actionable
  const cardBody =
    n.get_actions().length > 0 ? (
      <button
        onClicked={() => {
          if (n.get_actions()[0]) {
            n.invoke(n.get_actions()[0].id);
            onActionExecution();
          }
        }}
      >
        {content}
      </button>
    ) : (
      content
    );

  return (
    <box cssClasses={["notification-card", urgency(n)]} setup={setup}>
      <box vertical={true}>
        <box cssClasses={["header"]}>
          {n.appIcon || n.desktopEntry ? (
            <image
              cssClasses={["app-icon"]}
              visible={Boolean(n.appIcon || n.desktopEntry)}
              iconName={n.appIcon || n.desktopEntry}
            />
          ) : (
            <></>
          )}
          <label
            cssClasses={["app-name"]}
            halign={START}
            label={n.appName || ""}
          />
          <label
            cssClasses={["time"]}
            hexpand={true}
            halign={END}
            label={time(n.time)}
          />
          <button cssClasses={["closeButton"]} onClicked={() => n.dismiss()}>
            <image iconName="window-close-symbolic" />
          </button>
        </box>
        {cardBody}
        {n.get_actions().length > 1 && (
          <box cssClasses={["actions"]}>
            {n.get_actions().map(({ label, id }) => (
              <button hexpand={true} onClicked={() => n.invoke(id)}>
                <label label={label} halign={CENTER} hexpand={true} />
              </button>
            ))}
          </box>
        )}
      </box>
    </box>
  );
};
