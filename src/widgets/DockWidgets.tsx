import { invoke } from "@tauri-apps/api/core";
import { CheckMenuItem, Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
import { byKind, SIZE_LABEL } from "./registry";
import { sideOf, type WidgetInstance } from "./shared";

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

/** The widgets area of the dock: one tile per widget; click opens its expanded view. */
export function DockWidgets({ widgets }: { widgets: WidgetInstance[] }) {
  if (!widgets.length) return null;
  return (
    <div className="widgets">
      {widgets.map((w) => {
        const kind = byKind(w.kind);
        if (!kind) return null;
        const Tile = kind.Tile;
        return (
          <button
            key={w.id}
            className={`widget size-${w.size}`}
            onClick={(e) => {
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
          >
            <Tile instance={w} />
          </button>
        );
      })}
    </div>
  );
}
