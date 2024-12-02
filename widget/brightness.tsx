import GObject, { register, property, signal } from "astal/gobject";
import { makeProgressTile, percentageToIconFromList } from "./utils";
import { bind, exec, execAsync, monitorFile, Variable } from "astal";

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

export function Brightness(): JSX.Element {
  const brightness = BrilloObj.get_default();

  if (brightness.available) {
    let tile = bind(brightness, "screenValue").as((value) => ({
      icon: percentageToIconFromList(value, BRIGHTNESS_ICONS) || "",
      progress: value,
      visible: brightness.available,
    }));
    return makeProgressTile(tile);
  } else {
    return null;
  }
}
