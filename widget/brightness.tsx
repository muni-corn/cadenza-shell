import GObject, { register, property, signal } from "astal/gobject";
import { makeProgressTile, percentageToIconFromList } from "./utils";
import { bind, exec, execAsync, monitorFile, Variable } from "astal";

@register({ GTypeName: "BrilloObj" })
export class BrilloObj extends GObject.Object {
  // this Object assumes only one device with backlight
  #interface = exec("sh -c 'ls -w1 /sys/class/backlight | head -1'");

  #rawScreenValue = 0;
  #min = Number(exec("brillo -rc")) || 0;
  #max = Number(exec("brillo -rm")) || 1;

  @property(Boolean)
  get available() {
    return this.#interface.trim() !== "";
  }

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

    // setup monitor
    const brightness = `/sys/class/backlight/${this.#interface}/brightness`;
    monitorFile(brightness, () => this.#onChange());

    // initialize
    this.#onChange();
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
  const brightness = new BrilloObj();

  let tile = bind(brightness, "screenValue").as((value) => ({
    icon: percentageToIconFromList(value, BRIGHTNESS_ICONS) || "",
    progress: value,
    visible: brightness.available,
  }));

  return makeProgressTile(tile);
}
