import type { ComponentType } from "react";
import type { WidgetProps, WidgetSize } from "./shared";
import { SystemPanel, SystemTile } from "./system";
import { WeatherPanel, WeatherTile } from "./weather";
import { TodoPanel, TodoTile } from "./todo";
import { NotesPanel, NotesTile } from "./notes";
import { PomodoroPanel, PomodoroTile } from "./pomodoro";
import { WorldClockPanel, WorldClockTile } from "./worldclock";
import { AiPanel, AiTile } from "./ai";

/**
 * A widget = this manifest + a Tile (in the dock) + a Panel (the expanded view
 * in the popover) + optional Rust data commands.
 */
export type WidgetKind = {
  kind: string;
  name: string;
  group: "AI" | "Clocks" | "Productivity" | "System" | "Weather";
  description: string;
  sizes: WidgetSize[];
  defaultSize: WidgetSize;
  /** Expanded view size in CSS px. */
  panel: { width: number; height: number };
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  Tile: ComponentType<WidgetProps<any>>;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  Panel: ComponentType<WidgetProps<any>>;
};

export const WIDGETS: WidgetKind[] = [
  {
    kind: "ai",
    name: "AI usage",
    group: "AI",
    description: "Claude or OpenAI spend, DeepSeek balance, or Claude plan limits.",
    sizes: ["s", "m", "l"],
    defaultSize: "m",
    panel: { width: 350, height: 500 },
    Tile: AiTile,
    Panel: AiPanel,
  },
  {
    kind: "system",
    name: "System monitor",
    group: "System",
    description: "CPU, memory and network, with live charts.",
    sizes: ["s", "m", "l"],
    defaultSize: "m",
    panel: { width: 360, height: 400 },
    Tile: SystemTile,
    Panel: SystemPanel,
  },
  {
    kind: "weather",
    name: "Weather",
    group: "Weather",
    description: "Now and the next five days, from Open-Meteo.",
    sizes: ["s", "m", "l"],
    defaultSize: "m",
    panel: { width: 340, height: 250 },
    Tile: WeatherTile,
    Panel: WeatherPanel,
  },
  {
    kind: "todo",
    name: "To-do",
    group: "Productivity",
    description: "A quick task list, kept on this PC.",
    sizes: ["s", "m", "l"],
    defaultSize: "m",
    panel: { width: 330, height: 400 },
    Tile: TodoTile,
    Panel: TodoPanel,
  },
  {
    kind: "notes",
    name: "Notes",
    group: "Productivity",
    description: "A scratch pad that saves as you type.",
    sizes: ["s", "m", "l"],
    defaultSize: "s",
    panel: { width: 360, height: 340 },
    Tile: NotesTile,
    Panel: NotesPanel,
  },
  {
    kind: "pomodoro",
    name: "Pomodoro",
    group: "Productivity",
    description: "Focus and break timer with a chime.",
    sizes: ["s", "m", "l"],
    defaultSize: "m",
    panel: { width: 290, height: 270 },
    Tile: PomodoroTile,
    Panel: PomodoroPanel,
  },
  {
    kind: "worldclock",
    name: "World clock",
    group: "Clocks",
    description: "Times in other cities.",
    sizes: ["s", "m", "l"],
    defaultSize: "m",
    panel: { width: 330, height: 380 },
    Tile: WorldClockTile,
    Panel: WorldClockPanel,
  },
];

export const GROUPS = ["AI", "Clocks", "Productivity", "System", "Weather"] as const;

export const byKind = (kind: string) => WIDGETS.find((w) => w.kind === kind);

export const SIZE_LABEL: Record<WidgetSize, string> = { s: "Small", m: "Medium", l: "Large" };
