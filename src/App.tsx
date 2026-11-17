import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CheckMenuItem, Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
import { AppButton, useApps } from "./apps";
import { Clock, StartButton, SystemStrip, useSystemStatus } from "./system";
import { type Edge, updateSettings, useSettings } from "./settings";
import { DockWidgets } from "./widgets/DockWidgets";
import { sideOf } from "./widgets/shared";
import "./widgets/widgets.css";
import "./App.css";

function useClock() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(id);
  }, []);
  return now;
}

export default function App() {
  const settings = useSettings();
  const edge: Edge = settings?.edge ?? "bottom";
  const leftWidgets = (settings?.widgets ?? []).filter((w) => sideOf(w) === "left");
  const rightWidgets = (settings?.widgets ?? []).filter((w) => sideOf(w) === "right");
  const [dragging, setDragging] = useState(false);
  const dragStartY = useRef<number | null>(null);
  const barRef = useRef<HTMLDivElement>(null);

  // Tell Rust how wide the bar wants to be, so the dock window fits it exactly.
  useEffect(() => {
    const el = barRef.current;
    if (!el) return;
    const report = () => invoke("dock_set_bar_width", { width: Math.ceil(el.getBoundingClientRect().width) });
    const ro = new ResizeObserver(report);
    ro.observe(el);
    report();
    return () => ro.disconnect();
  }, []);
  const draggedPin = useRef<string | null>(null);
  const now = useClock();
  const apps = useApps();
  const status = useSystemStatus();

  function dropOnPin(targetKey: string) {
    const from = draggedPin.current;
    draggedPin.current = null;
    if (!from || from === targetKey) return;
    const keys = apps.filter((a) => a.pinned).map((a) => a.key);
    keys.splice(keys.indexOf(from), 1);
    keys.splice(keys.indexOf(targetKey), 0, from);
    invoke("pins_reorder", { keys }).catch(console.error);
  }

  function moveTo(target: Edge) {
    if (target === edge) return;
    updateSettings({ edge: target }).catch(console.error);
  }

  async function onContextMenu(e: React.MouseEvent) {
    e.preventDefault();
    if (!settings) return;
    const mode = settings.taskbar_mode;
    const modeItem = async (value: typeof mode, text: string) =>
      CheckMenuItem.new({
        text,
        checked: mode === value,
        action: () => updateSettings({ taskbar_mode: value }).catch(console.error),
      });
    const menu = await Menu.new({
      items: [
        await MenuItem.new({
          text: "Widgets…",
          action: () => invoke("open_widget_picker"),
        }),
        await MenuItem.new({
          text: "Settings…",
          action: () => invoke("open_settings"),
        }),
        await PredefinedMenuItem.new({ item: "Separator" }),
        await modeItem("hide", "Windows taskbar: auto-hide"),
        await modeItem("complete", "Windows taskbar: hide completely"),
        await modeItem("leave", "Windows taskbar: don't hide"),
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
        ref={barRef}
        className={`bar${dragging ? " dragging" : ""}`}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
      >
        <StartButton />
        <span className="divider" />
        {leftWidgets.length > 0 && (
          <>
            <DockWidgets widgets={leftWidgets} />
            <span className="divider" />
          </>
        )}
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
        {rightWidgets.length > 0 && (
          <>
            <span className="divider" />
            <DockWidgets widgets={rightWidgets} />
          </>
        )}
        <span className="divider" />
        <SystemStrip status={status} />
        <Clock now={now} />
      </div>
    </div>
  );
}
