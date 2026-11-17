import { invoke } from "@tauri-apps/api/core";
import { CheckMenuItem, Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
import { byKind, SIZE_LABEL } from "./registry";
import { useState } from "react";
import { placeWidget } from "./arrange";
import { sideOf, type WidgetInstance, type WidgetSide } from "./shared";

async function widgetMenu(w: WidgetInstance) {
  const kind = byKind(w.kind);
  if (!kind) return;
  const items = [];
  for (const size of kind.sizes) {
    items.push(
      await CheckMenuItem.new({
        text: SIZE_LABEL[size],
        checked: w.size === size,
        action: () => invoke("widget_update", { id: w.id, size }),
      }),
    );
  }
  const other = sideOf(w) === "left" ? "right" : "left";
  items.push(
    await PredefinedMenuItem.new({ item: "Separator" }),
    await MenuItem.new({
      text: `Move to the ${other} of the apps`,
      action: () => invoke("widget_update", { id: w.id, side: other }),
    }),
    await MenuItem.new({ text: `Remove ${kind.name}`, action: () => invoke("widgets_remove", { id: w.id }) }),
    await MenuItem.new({ text: "Widgets…", action: () => invoke("open_widget_picker") }),
  );
  await (await Menu.new({ items })).popup();
}

// Shared across both sides' groups, so a tile can be dragged from one to the other.
let draggedId: string | null = null;

// Hover glance: details open after the pointer rests on a widget briefly.
let glanceTimer: number | undefined;

function glanceEnter(el: HTMLElement, w: WidgetInstance, panel: { width: number; height: number }) {
  window.clearTimeout(glanceTimer);
  const r = el.getBoundingClientRect();
  glanceTimer = window.setTimeout(() => {
    invoke("popover_glance", { id: w.id, anchorX: r.left + r.width / 2, width: panel.width, height: panel.height }).catch(console.error);
  }, 450);
}

function glanceLeave() {
  window.clearTimeout(glanceTimer);
  invoke("popover_hover", { source: "dock", inside: false });
}

/**
 * One side's widget group in the dock. Normally a click opens a widget's
 * expanded view. In edit mode (while the Widgets window is open) the tiles can
 * be dragged — within the side or across the apps to the other side — and each
 * has a remove badge.
 */
export function DockWidgets({
  widgets,
  all,
  side,
  edit,
}: {
  widgets: WidgetInstance[];
  all: WidgetInstance[];
  side: WidgetSide;
  edit: boolean;
}) {
  const [over, setOver] = useState<string | null>(null);
  if (!widgets.length && !edit) return null;

  const drop = (beforeId?: string) => {
    const id = draggedId;
    draggedId = null;
    setOver(null);
    if (id && id !== beforeId) placeWidget(all, id, side, beforeId);
  };

  return (
    <div
      className={`widgets${edit ? " editing" : ""}${over === "end" ? " drop-end" : ""}`}
      onDragOver={(e) => {
        if (!edit) return;
        e.preventDefault();
        if (!over) setOver("end");
      }}
      onDragLeave={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node)) setOver(null);
      }}
      onDrop={(e) => {
        e.preventDefault();
        drop();
      }}
    >
      {widgets.map((w) => {
        const kind = byKind(w.kind);
        if (!kind) return null;
        const Tile = kind.Tile;
        return (
          <button
            key={w.id}
            className={`widget size-${w.size} kind-${w.kind}${over === w.id ? " drop-before" : ""}`}
            draggable={edit}
            title={edit ? `${kind.name} — drag to move, across the apps to switch sides` : undefined}
            onMouseEnter={(e) => !edit && glanceEnter(e.currentTarget, w, kind.panel)}
            onMouseLeave={() => !edit && glanceLeave()}
            onClick={(e) => {
              if (edit) return;
              window.clearTimeout(glanceTimer);
              const r = e.currentTarget.getBoundingClientRect();
              invoke("popover_toggle", {
                id: w.id,
                anchorX: r.left + r.width / 2,
                width: kind.panel.width,
                height: kind.panel.height,
              }).catch(console.error);
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
              widgetMenu(w).catch(console.error);
            }}
            onDragStart={(e) => {
              draggedId = w.id;
              e.dataTransfer.effectAllowed = "move";
              e.dataTransfer.setData("text/plain", w.id);
            }}
            onDragEnd={() => {
              draggedId = null;
              setOver(null);
            }}
            onDragOver={(e) => {
              if (!edit) return;
              e.preventDefault();
              e.stopPropagation();
              setOver(w.id);
            }}
            onDrop={(e) => {
              e.preventDefault();
              e.stopPropagation();
              drop(w.id);
            }}
          >
            <Tile instance={w} />
            {edit && (
              <span
                className="w-remove"
                role="button"
                title={`Remove ${kind.name}`}
                onClick={(e) => {
                  e.stopPropagation();
                  invoke("widgets_remove", { id: w.id });
                }}
              >
                {""}
              </span>
            )}
          </button>
        );
      })}
      {edit && widgets.length === 0 && <div className="drop-slot">Drop here</div>}
    </div>
  );
}
