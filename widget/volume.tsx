import Wp from "gi://AstalWp";
import Gtk from "gi://Gtk";
import { Variable, bind, timeout } from "astal";
import { ProgressBar } from "./progress.tsx";

const VOLUME_ICONS = ["\u{F057F}", "\u{F0580}", "\u{F057E}"];
const MUTE_ICON = "\u{F0581}";
const ZERO_ICON = "\u{F0E08}";

export const Volume = () => {
  const audio = Wp.get_default();

  if (audio) {
    const volume = bind(audio.default_speaker, "volume");
    const mute = bind(audio.default_speaker, "mute");
    const state = Variable.derive([volume, mute], (volume, mute) => ({
      volume,
      mute,
    }))();

    // for fade effects
    let lastChangeTime = 0;
    const extraClasses: Variable<"dim" | "bright"> = Variable("dim");
    state.subscribe(() => {
      // because `icon` reacts to changes to both `volume` and `mute`, we
      // can just reuse its binding to make fade animations
      extraClasses.set("bright");
      lastChangeTime = Date.now();

      timeout(3000, () => {
        if (Date.now() - lastChangeTime >= 3000) {
          extraClasses.set("dim");
        }
      });
    });

    return (
      <box spacing={8}>
        <label
          label={state.as(getIcon)}
          cssClasses={extraClasses((c) => ["icon", c])}
          widthRequest={16}
        />
        <ProgressBar
          cssClasses={extraClasses((c) => [c])}
          fraction={volume}
          valign={Gtk.Align.CENTER}
          widthRequest={16}
        />
      </box>
    );
  }
  return <label label="No audio" cssClasses={["dim"]} />;
};

function getIcon({ volume, mute }: { volume: number; mute: boolean }): string {
  if (mute) {
    return MUTE_ICON;
  }
  if (volume === 0) {
    return ZERO_ICON;
  }
  const index = Math.floor(volume * VOLUME_ICONS.length);
  return VOLUME_ICONS[Math.min(index, VOLUME_ICONS.length - 1)];
}
