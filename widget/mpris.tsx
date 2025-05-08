import Mpris from "gi://AstalMpris";
import AstalMpris from "gi://AstalMpris?version=0.1";
import { bind } from "astal";
import { trunc } from "./utils";

const mpris = Mpris.get_default();

const MPRIS_PLAYING_ICON = "\u{F0F74}";
const MPRIS_PAUSED_ICON = "\u{F03E4}";

export function Media(): JSX.Element {
  return (
    <>
      {bind(mpris, "players").as((players) => {
        const player =
          players.find(
            (p) => p.playback_status === AstalMpris.PlaybackStatus.PLAYING,
          ) || players[0];

        if (player) {
          const icon = bind(player, "playback_status").as((s) =>
            s === AstalMpris.PlaybackStatus.PLAYING
              ? MPRIS_PLAYING_ICON
              : MPRIS_PAUSED_ICON,
          );
          const title = bind(player, "title").as(
            (t) =>
              trunc(t) || `Media is ${statusToString(player.playback_status)}`,
          );
          const artist = bind(player, "artist").as((a) => trunc(a) || "");
          const visible = bind(player, "playback_status").as(
            (s) => s !== AstalMpris.PlaybackStatus.STOPPED,
          );

          return (
            <box spacing={12} visible={visible}>
              <label label={icon} className={"icon"} widthRequest={16} />
              <label label={title} className={"primary"} />
              <label label={artist} className={"secondary"} />
            </box>
          );
        }
        return <></>;
      })}
    </>
  );
}

function statusToString(status: Mpris.PlaybackStatus) {
  switch (status) {
    case Mpris.PlaybackStatus.PLAYING:
      return "playing";
    case Mpris.PlaybackStatus.PAUSED:
      return "paused";
    case Mpris.PlaybackStatus.STOPPED:
      return "stopped";
  }
}
