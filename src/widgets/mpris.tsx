import Mpris from "gi://AstalMpris";
import AstalMpris from "gi://AstalMpris?version=0.1";
import { createBinding, With } from "ags";
import { trunc } from "./utils";

const mpris = Mpris.get_default();

const MPRIS_PLAYING_ICON = "\u{F0F74}";
const MPRIS_PAUSED_ICON = "\u{F03E4}";

export const Media = () => {
  const players = createBinding(mpris, "players");

  return (
    <With value={players}>
      {(players) => {
        const player =
          players.find(
            (p) => p.playback_status === AstalMpris.PlaybackStatus.PLAYING,
          ) || players[0];

        if (player) {
          const icon = createBinding(player, "playback_status").as((s) =>
            s === AstalMpris.PlaybackStatus.PLAYING
              ? MPRIS_PLAYING_ICON
              : MPRIS_PAUSED_ICON,
          );
          const title = createBinding(player, "title").as(
            (t) =>
              trunc(t) || `Media is ${statusToString(player.playback_status)}`,
          );
          const artist = createBinding(player, "artist").as(
            (a) => trunc(a) || "",
          );
          const visible = createBinding(player, "playback_status").as(
            (s) => s !== AstalMpris.PlaybackStatus.STOPPED,
          );

          return (
            <box spacing={12} visible={visible}>
              <label label={icon} cssClasses={["icon"]} widthRequest={16} />
              <label label={title} cssClasses={["primary"]} />
              <label label={artist} cssClasses={["secondary"]} />
            </box>
          );
        }
      }}
    </With>
  );
};

function statusToString(status: Mpris.PlaybackStatus) {
  switch (status) {
    case Mpris.PlaybackStatus.PLAYING:
      return "playing";
    case Mpris.PlaybackStatus.PAUSED:
      return "paused";
    case Mpris.PlaybackStatus.STOPPED:
      return "stopped";
    default:
      return "unknown";
  }
}
