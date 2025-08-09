import Notifd from "gi://AstalNotifd";
import GLib from "gi://GLib?version=2.0";
import { Gdk, Gtk } from "ags/gtk4";

const isIcon = (icon: string) => {
  const iconTheme = Gtk.IconTheme.get_for_display(Gdk.Display.get_default()!);
  return icon && iconTheme.has_icon(icon);
};

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
  setup?: (self: Gtk.Widget) => void;
  afterActionExecution?: () => void;
  notification: Notifd.Notification;
};

export const NotificationCard = (props: Props) => {
  const { notification: n, setup, afterActionExecution } = props;
  const { START, CENTER, END } = Gtk.Align;

  const content = (
    <box class="content" hexpand>
      {n.image &&
        (fileExists(n.image) ? (
          <image valign={START} class="image" file={n.image} />
        ) : (
          <box hexpand={false} valign={START} class="icon-image">
            <image
              iconName={n.image}
              hexpand={true}
              vexpand={true}
              halign={CENTER}
              valign={CENTER}
            />
          </box>
        ))}
      <box orientation={Gtk.Orientation.VERTICAL}>
        <label
          class="summary"
          wrap={true}
          halign={START}
          xalign={0}
          label={n.summary}
          lines={2}
          ellipsize={3}
          maxWidthChars={1}
          hexpand
        />
        {n.body && (
          <label
            class="body"
            wrap={true}
            useMarkup={true}
            halign={START}
            xalign={0}
            label={n.body}
            lines={4}
            ellipsize={3}
            maxWidthChars={1}
            hexpand
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
            afterActionExecution?.();
          }
        }}
      >
        {content}
      </button>
    ) : (
      content
    );

  return (
    <box cssClasses={["notification-card", urgency(n)]} $={setup}>
      <box orientation={Gtk.Orientation.VERTICAL}>
        <box class="header">
          {n.appIcon || isIcon(n.desktopEntry) ? (
            <image class="app-icon" iconName={n.appIcon || n.desktopEntry} />
          ) : null}
          {n.appName && (
            <label class="app-name" halign={START} label={n.appName} />
          )}
          <label
            class="time"
            hexpand={true}
            halign={END}
            label={time(n.time)}
          />
          <button class="closeButton" onClicked={() => n.dismiss()}>
            <image iconName="window-close-symbolic" />
          </button>
        </box>
        {cardBody}
        {n.get_actions().length > 1 && (
          <box class="actions">
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
