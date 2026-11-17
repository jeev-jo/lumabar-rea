import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { sideOf, type WidgetInstance, type WidgetSide } from "./shared";

/**
 * Move widget `id` to `side`, just before `beforeId` (or to the end of that side),
 * and save the new order. Left widgets always come first in the stored order.
 */
export function placeWidget(all: WidgetInstance[], id: string, side: WidgetSide, beforeId?: string) {
  const moving = all.find((w) => w.id === id);
  if (!moving) return;
  const rest = all.filter((w) => w.id !== id);
  const order = [...rest.filter((w) => sideOf(w) === "left"), ...rest.filter((w) => sideOf(w) === "right")].map((w) => w.id);
  let at: number;
  if (beforeId && beforeId !== id && order.includes(beforeId)) {
    at = order.indexOf(beforeId);
  } else {
    const lastOfSide = [...rest].reverse().find((w) => sideOf(w) === side);
    at = lastOfSide ? order.indexOf(lastOfSide.id) + 1 : side === "left" ? 0 : order.length;
  }
  order.splice(at, 0, id);
  if (sideOf(moving) !== side) invoke("widget_update", { id, side });
  invoke("widgets_reorder", { ids: order });
}

/** True while the Widgets window is open: the dock's widgets can then be arranged. */
export function useEditMode() {
  const [on, setOn] = useState(false);
  useEffect(() => {
    // Listen first, then ask: a change between the two can't be missed.
    const un = listen<boolean>("edit-mode", (e) => setOn(e.payload));
    un.then(() => invoke<boolean>("widgets_edit_mode").then(setOn)).catch(() => {});
    return () => {
      un.then((f) => f());
    };
  }, []);
  return on;
}
