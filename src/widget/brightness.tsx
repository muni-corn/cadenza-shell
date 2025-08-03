import Gtk from "gi://Gtk";
import { createBinding, createState } from "ags";
import { monitorFile } from "ags/file";
import GObject, { getter, property, register, setter } from "ags/gobject";
import { exec, execAsync } from "ags/process";
import { timeout } from "ags/time";
import { percentageToIconFromList } from "./utils";

@register({ GTypeName: "BrilloObj" })
export class BrilloObj extends GObject.Object {
  static instance: BrilloObj;
  static get_default() {
    if (!BrilloObj.instance) {
      BrilloObj.instance = new BrilloObj();
    }
    return BrilloObj.instance;
  }

  // this Object assumes only one device with backlight
  #rawScreenValue = 0;

  #interface = "";
  #min = 0;
  #max = 0;

  @property(Boolean)
  available: boolean = false;

  @getter(Number)
  get screenValue() {
    return (this.#rawScreenValue - this.#min) / (this.#max - this.#min);
  }

  @setter(Number)
  set screenValue(percent) {
    let rawValue = this.#min + (this.#max - this.#min) * percent;
    if (rawValue < this.#min) {
      rawValue = this.#min;
    } else if (rawValue > this.#max) {
      rawValue = this.#max;
    }

    execAsync(`brillo -Sr ${rawValue}`);

    // the file monitor will handle calling the signal
  }

  constructor() {
    super();

    try {
      this.#interface = exec("sh -c 'ls -w1 /sys/class/backlight | head -1'");
      this.#min = Number(exec("brillo -rc")) || 0;
      this.#max = Number(exec("brillo -rm")) || 1;
      this.available = true;
    } catch (_e) {
      this.available = false;
    }

    if (this.available) {
      // setup monitor
      const brightness = `/sys/class/backlight/${this.#interface}/brightness`;
      monitorFile(brightness, () => this.#onChange());

      // initialize
      this.#onChange();
    }
  }

  #onChange() {
    this.#rawScreenValue = Number(exec("brillo -rG")) || 0;
    this.notify("screen-value");
  }
}

const BRIGHTNESS_ICONS = [
  "\u{F00DB}",
  "\u{F00DC}",
  "\u{F00DD}",
  "\u{F00DE}",
  "\u{F00DF}",
  "\u{F00E0}",
];

export const Brightness = () => {
  const brightness = BrilloObj.get_default();

  // for fade effects
  let lastChangeTime = 0;
  const [extraClasses, setExtraClasses] = createState<"dim" | "bright">("dim");

  createBinding(brightness, "screenValue").subscribe(() => {
    setExtraClasses("bright");
    lastChangeTime = Date.now();

    timeout(3000, () => {
      if (Date.now() - lastChangeTime >= 3000) {
        setExtraClasses("dim");
      }
    });
  });

  const screenValue = createBinding(brightness, "screenValue");

  return (
    <box spacing={8} visible={brightness.available}>
      <label
        label={screenValue.as(
          (v) => percentageToIconFromList(v, BRIGHTNESS_ICONS) || "",
        )}
        cssClasses={extraClasses((c) => ["icon", c])}
        widthRequest={16}
      />
      <Gtk.ProgressBar
        cssClasses={extraClasses((c) => [c])}
        fraction={screenValue}
        valign={Gtk.Align.CENTER}
        widthRequest={16}
      />
    </box>
  );
};
