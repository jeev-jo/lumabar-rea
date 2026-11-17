import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CheckMenuItem, Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
import { AppButton, useApps } from "./apps";
import "./App.css";

type Edge = "top" | "bottom";
type TaskbarMode = "hide" | "leave";
type Settings = { edge: Edge; taskbar_mode: TaskbarMode };

function useClock() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(id);
  }, []);
  return now;
}

export default function App() {
  const [edge, setEdge] = useState<Edge>("bottom");
  const [dragging, setDragging] = useState(false);
  const dragStartY = useRef<number | null>(null);
  const draggedPin = useRef<string | null>(null);
  const now = useClock();
  const apps = useApps();

  function dropOnPin(targetKey: string) {
    const from = draggedPin.current;
    draggedPin.current = null;
    if (!from || from === targetKey) return;
    const keys = apps.filter((a) => a.pinned).map((a) => a.key);
    keys.splice(keys.indexOf(from), 1);
    keys.splice(keys.indexOf(targetKey), 0, from);
    invoke("pins_reorder", { keys }).catch(console.error);
  }

  useEffect(() => {
    invoke<Settings>("get_settings")
      .then((s) => setEdge(s.edge))
      .catch(console.error);
  }, []);

  function moveTo(target: Edge) {
    if (target === edge) return;
    setEdge(target);
    invoke("dock_set_edge", { edge: target }).catch(console.error);
  }

  async function onContextMenu(e: React.MouseEvent) {
    e.preventDefault();
    const settings = await invoke<Settings>("get_settings");
    const menu = await Menu.new({
      items: [
        await CheckMenuItem.new({
          text: "Hide the Windows taskbar",
          checked: settings.taskbar_mode === "hide",
          action: () =>
            invoke("set_taskbar_mode", {
              mode: settings.taskbar_mode === "hide" ? "leave" : "hide",
            }).catch(console.error),
        }),
        await PredefinedMenuItem.new({ item: "Separator" }),
        await MenuItem.new({
          text: edge === "bottom" ? "Move dock to top" : "Move dock to bottom",
          action: () => moveTo(edge === "bottom" ? "top" : "bottom"),
        }),
        await PredefinedMenuItem.new({ item: "Separator" }),
        await MenuItem.new({
          text: "Quit  (Ctrl+Alt+Shift+Q)",
          action: () => invoke("quit_app"),
        }),
      ],
    });
    await menu.popup();
  }

  // Drag the bar towards the other screen edge and let go: it snaps there.
  function onPointerDown(e: React.PointerEvent<HTMLDivElement>) {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button, .app")) return;
    dragStartY.current = e.screenY;
    e.currentTarget.setPointerCapture(e.pointerId);
  }
  function onPointerMove(e: React.PointerEvent<HTMLDivElement>) {
    if (dragStartY.current === null) return;
    if (Math.abs(e.screenY - dragStartY.current) > 24) setDragging(true);
  }
  function onPointerUp(e: React.PointerEvent<HTMLDivElement>) {
    if (dragStartY.current === null) return;
    const moved = Math.abs(e.screenY - dragStartY.current) > 24;
    dragStartY.current = null;
    setDragging(false);
    if (!moved) return;
    moveTo(e.screenY < window.screen.height / 2 ? "top" : "bottom");
  }

  return (
    <div className={`strip edge-${edge}`} onContextMenu={onContextMenu}>
      <div
        className={`bar${dragging ? " dragging" : ""}`}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
      >
        <div className="apps">
          {apps.length === 0 && <span className="brand">luma</span>}
          {apps.map((app) => (
            <AppButton
              key={app.key}
              app={app}
              onDragStartPin={(k) => (draggedPin.current = k)}
              onDropOnPin={dropOnPin}
            />
          ))}
        </div>
        <span className="divider" />
        <span className="clock">
          {now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
          <small>{now.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" })}</small>
        </span>
        <button className="quit" title="Quit luma-ai-bar" onClick={() => invoke("quit_app")}>
          ×
        </button>
      </div>
    </div>
  );
}
