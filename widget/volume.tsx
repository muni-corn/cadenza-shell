import { bind, Variable } from "astal";
import { ProgressTile, makeProgressTile } from "./utils";
import Wp from "gi://AstalWp";

const VOLUME_ICONS = ["\u{F057F}", "\u{F0580}", "\u{F057E}"];
const MUTE_ICON = "\u{F0581}";
const ZERO_ICON = "\u{F0E08}";

export function Volume(): JSX.Element {
  const audio = Wp.get_default();

  if (audio) {
    const getIcon = (speaker: Wp.Endpoint): string => {
      if (speaker.mute) {
        return MUTE_ICON;
      } else if (speaker.volume === 0) {
        return ZERO_ICON;
      } else {
        let index = Math.floor(speaker.volume * VOLUME_ICONS.length);
        return VOLUME_ICONS[Math.min(index, VOLUME_ICONS.length - 1)];
      }
    };

    const getProgressTile = (speaker: Wp.Endpoint): ProgressTile => ({
      icon: getIcon(speaker),
      progress: speaker.volume || 0,
      visible: true,
    });

    const tile = Variable.derive(
      [
        bind(audio.default_speaker, "volume"),
        bind(audio.default_speaker, "mute"),
      ],
      () => getProgressTile(audio.default_speaker),
    );

    return makeProgressTile(tile());
  } else {
    return <label label="No audio" className="dim" />;
  }
}
