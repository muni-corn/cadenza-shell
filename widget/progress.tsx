import { type ConstructProps, Gtk, astalify } from "astal/gtk4";

export type ProgressBarProps = ConstructProps<
  Gtk.ProgressBar,
  Gtk.ProgressBar.ConstructorProps
>;
export const ProgressBar = astalify<
  Gtk.ProgressBar,
  Gtk.ProgressBar.ConstructorProps
>(Gtk.ProgressBar, {});
