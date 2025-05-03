import Notifd from "gi://AstalNotifd";
import { GLib } from "astal";
import { Astal, Gtk } from "astal/gtk3";
import type { EventBox } from "astal/gtk3/widget";

const isIcon = (icon: string) => !!Astal.Icon.lookup_icon(icon);

const fileExists = (path: string) => GLib.file_test(path, GLib.FileTest.EXISTS);

const time = (time: number, format = "%-I:%M %P") =>
  GLib.DateTime.new_from_unix_local(time).format(format)!;

const urgency = (n: Notifd.Notification) => {
  const { LOW, NORMAL, CRITICAL } = Notifd.Urgency;
  // match operator when?
  switch (n.urgency) {
    case LOW:
      return "low";
    case CRITICAL:
      return "critical";
    case NORMAL:
    default:
      return "normal";
  }
};

type Props = {
  setup(self: EventBox): void;
  onActionExecution(): void;
  notification: Notifd.Notification;
};

export default function NotificationCard(props: Props) {
  const { notification: n, setup, onActionExecution } = props;
  const { START, CENTER, END } = Gtk.Align;

  const content = (
    <box className="content">
      {n.image && fileExists(n.image) && (
        <box
          valign={START}
          className="image"
          css={`
            background-image: url("${n.image}");
          `}
        />
      )}
      {n.image && isIcon(n.image) && (
        <box expand={false} valign={START} className="icon-image">
          <icon icon={n.image} expand={true} halign={CENTER} valign={CENTER} />
        </box>
      )}
      <box vertical={true}>
        <label
          className="summary"
          wrap={true}
          halign={START}
          xalign={0}
          label={n.summary}
          truncate={true}
          lines={2}
        />
        {n.body && (
          <label
            className="body"
            wrap={true}
            useMarkup={true}
            halign={START}
            xalign={0}
            label={n.body}
            truncate={true}
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
    <eventbox className={`notification-card ${urgency(n)}`} setup={setup}>
      <box vertical={true}>
        <box className="header">
          {(n.appIcon || n.desktopEntry) && (
            <icon
              className="app-icon"
              visible={Boolean(n.appIcon || n.desktopEntry)}
              icon={n.appIcon || n.desktopEntry}
            />
          )}
          <label
            className="app-name"
            halign={START}
            truncate={true}
            label={n.appName || ""}
          />
          <label
            className="time"
            hexpand={true}
            halign={END}
            label={time(n.time)}
          />
          <button className="closeButton" onClicked={() => n.dismiss()}>
            <icon icon="window-close-symbolic" />
          </button>
        </box>
        {cardBody}
        {n.get_actions().length > 1 && (
          <box className="actions">
            {n.get_actions().map(({ label, id }) => (
              <button hexpand={true} onClicked={() => n.invoke(id)}>
                <label label={label} halign={CENTER} hexpand={true} />
              </button>
            ))}
          </box>
        )}
      </box>
    </eventbox>
  );
}
