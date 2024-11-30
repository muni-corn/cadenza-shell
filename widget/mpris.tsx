import { makeTile } from "./utils";
import { Variable, bind } from "astal";
import Mpris from "gi://AstalMpris";

const mpris = Mpris.get_default();

const MPRIS_PLAYING_ICON = "\u{F0F74}";
const MPRIS_PAUSED_ICON = "\u{F03E4}";

export function Media() {
  const fallback = {
    icon: MPRIS_PAUSED_ICON,
    primary: "No players found",
    secondary: "",
    visible: false,
  };

  const tile = bind(mpris, "players").as((players) => {
    if (players[0]) {
      const { playback_status, artist, title } = players[0];
      return {
        icon:
          playback_status === Mpris.PlaybackStatus.PLAYING
            ? MPRIS_PLAYING_ICON
            : MPRIS_PAUSED_ICON,
        primary: title || `Media is ${statusToString(playback_status)}`,
        secondary: artist,
        visible: playback_status !== Mpris.PlaybackStatus.STOPPED,
      };
    } else {
      return fallback;
    }
  });

  return makeTile(tile);
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
