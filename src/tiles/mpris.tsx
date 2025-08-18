import Mpris from "gi://AstalMpris";
import AstalMpris from "gi://AstalMpris?version=0.1";
import { createBinding, createComputed, With } from "ags";
import { Tile, trunc } from "../utils";

const mpris = Mpris.get_default();

const MPRIS_PLAYING_ICON = "\u{F0F74}";
const MPRIS_PAUSED_ICON = "\u{F03E4}";

export const Media = () => {
  const players = createBinding(mpris, "players");

  const activePlayer =
    players
      .get()
      .find((p) => p.playback_status === AstalMpris.PlaybackStatus.PLAYING) ||
    players.get()[0];

  const data = createComputed(
    [
      players,
      createBinding(activePlayer, "playbackStatus"),
      createBinding(activePlayer, "title"),
      createBinding(activePlayer, "artist"),
    ],
    (players, playbackStatus, title, artist) => {
      return players.length > 0 && activePlayer
        ? {
            icon:
              playbackStatus === AstalMpris.PlaybackStatus.PLAYING
                ? MPRIS_PLAYING_ICON
                : MPRIS_PAUSED_ICON,
            title: trunc(title) || `Media is ${statusToString(playbackStatus)}`,
            artist: trunc(artist) || "",
            visible: playbackStatus !== AstalMpris.PlaybackStatus.STOPPED,
          }
        : {
            visible: false,
          };
    },
  );

  return (
    <With value={data}>
      {(data) => (
        <Tile
          visible={data.visible}
          icon={data.icon}
          primary={data.title}
          secondary={data.artist}
        />
      )}
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
