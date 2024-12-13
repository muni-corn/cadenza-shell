import { GLib } from "astal";
import { Gtk, Astal } from "astal/gtk3";
import { type EventBox } from "astal/gtk3/widget";
import Notifd from "gi://AstalNotifd";

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
          <icon icon={n.image} expand halign={CENTER} valign={CENTER} />
        </box>
      )}
      <box vertical>
        <label
          className="summary"
          wrap
          halign={START}
          xalign={0}
          label={n.summary}
          truncate
          lines={2}
        />
        {n.body && (
          <label
            className="body"
            wrap
            useMarkup
            halign={START}
            xalign={0}
            label={n.body}
            truncate
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
      <box vertical>
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
            truncate
            label={n.appName || ""}
          />
          <label className="time" hexpand halign={END} label={time(n.time)} />
          <button className="closeButton" onClicked={() => n.dismiss()}>
            <icon icon="window-close-symbolic" />
          </button>
        </box>
        {cardBody}
        {n.get_actions().length > 1 && (
          <box className="actions">
            {n.get_actions().map(({ label, id }) => (
              <button hexpand onClicked={() => n.invoke(id)}>
                <label label={label} halign={CENTER} hexpand />
              </button>
            ))}
          </box>
        )}
      </box>
    </eventbox>
  );
}
