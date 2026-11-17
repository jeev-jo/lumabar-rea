import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { WidgetInstance } from "./widgets/shared";
import "./theme.css";

export type Edge = "top" | "bottom";
export type TaskbarMode = "hide" | "complete" | "leave";
export type Size = "small" | "medium" | "large";
export type Theme = "system" | "dark" | "light";

export type Settings = {
  edge: Edge;
  taskbar_mode: TaskbarMode;
  size: Size;
  theme: Theme;
  display: string | null;
  widgets: WidgetInstance[];
};

export type Patch = Partial<Omit<Settings, "display" | "widgets">> & { display?: string };

export const updateSettings = (patch: Patch) => invoke<Settings>("update_settings", { patch });

function apply(s: Settings) {
  document.documentElement.dataset.theme = s.theme;
  document.documentElement.dataset.size = s.size;
}

/** Current settings, kept live across windows; also applies theme and size to this page. */
export function useSettings() {
  const [settings, setSettings] = useState<Settings | null>(null);
  useEffect(() => {
    invoke<Settings>("get_settings")
      .then((s) => {
        apply(s);
        setSettings(s);
      })
      .catch(console.error);
    const un = listen<Settings>("settings-changed", (e) => {
      apply(e.payload);
      setSettings(e.payload);
    });
    return () => {
      un.then((f) => f());
    };
  }, []);
  return settings;
}
