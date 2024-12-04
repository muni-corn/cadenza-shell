import GObject, { register, property } from "astal/gobject";
import { percentageToIconFromList } from "./utils";
import { bind, exec, execAsync, monitorFile, timeout, Variable } from "astal";
import { ProgressBar } from "./progress";
import Gtk from "gi://Gtk";

@register({ GTypeName: "BrilloObj" })
export class BrilloObj extends GObject.Object {
  static instance: BrilloObj;
  static get_default() {
    if (!this.instance) this.instance = new BrilloObj();
    return this.instance;
  }

  // this Object assumes only one device with backlight
  #rawScreenValue = 0;

  #interface: string = "";
  #min: number = 0;
  #max: number = 0;

  @property(Boolean)
  declare available: boolean;

  @property(Number)
  get screenValue() {
    return (this.#rawScreenValue - this.#min) / (this.#max - this.#min);
  }

  set screenValue(percent) {
    let raw_value = this.#min + (this.#max - this.#min) * percent;
    if (raw_value < this.#min) raw_value = this.#min;
    else if (raw_value > this.#max) raw_value = this.#max;

    execAsync(`brillo -Sr ${raw_value}`);

    // the file monitor will handle calling the signal
  }

  constructor() {
    super();

    try {
      this.#interface = exec("sh -c 'ls -w1 /sys/class/backlight | head -1'");
      this.#min = Number(exec("brillo -rc")) || 0;
      this.#max = Number(exec("brillo -rm")) || 1;
      this.available = true;
    } catch (e) {
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

export function Brightness(): JSX.Element | null {
  const brightness = BrilloObj.get_default();

    // for fade effects
  let lastChangeTime = 0;
  let extraClasses: Variable<"dim" | "bright"> = Variable("dim");
  bind(brightness, "screenValue").subscribe(() => {
    extraClasses.set("bright");
    lastChangeTime = Date.now();

    timeout(3000, () => {
      if (Date.now() - lastChangeTime >= 3000) {
        extraClasses.set("dim");
      }
    });
  });

  let screenValue = bind(brightness, "screenValue");

  return (
    <box spacing={8} visible={brightness.available}>
      <label
        label={screenValue.as(
          (v) => percentageToIconFromList(v, BRIGHTNESS_ICONS) || "",
        )}
        className={extraClasses((c) => `icon ${c}`)}
        widthRequest={16}
      />
      <ProgressBar
        className={extraClasses()}
        fraction={screenValue}
        valign={Gtk.Align.CENTER}
        widthRequest={16}
      />
    </box>
  );
}
