import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CheckMenuItem, Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
import { AppButton, useApps } from "./apps";
import { Clock, StartButton } from "./system";
import { type Edge, updateSettings, useSettings } from "./settings";
import { DockWidgets } from "./widgets/DockWidgets";
import { sideOf } from "./widgets/shared";
import { useEditMode } from "./widgets/arrange";
import "./widgets/widgets.css";
import { LiquidGlass } from "./liquid";
import { useUpdateCheck } from "./Updates";
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
  const allWidgets = settings?.widgets ?? [];
  const leftWidgets = allWidgets.filter((w) => sideOf(w) === "left");
  const rightWidgets = allWidgets.filter((w) => sideOf(w) === "right");
  const editing = useEditMode();
  const [dragging, setDragging] = useState(false);
  const dragStartY = useRef<number | null>(null);
  const barRef = useRef<HTMLDivElement>(null);
  // Below 1 when everything wouldn't fit on the screen: the bar's contents
  // shrink together (icons, widgets, clock), like a dock that runs out of room.
  const [fit, setFit] = useState(1);
  const fitRef = useRef(1);

  // Tell Rust how wide the bar wants to be, so the dock window fits it exactly.
  useEffect(() => {
    const el = barRef.current;
    if (!el) return;
    const report = () => {
      const width = el.getBoundingClientRect().width;
      const parts = [...el.children].filter((c) => !c.classList.contains("liquid-canvas"));
      const scaled = parts.reduce((sum, c) => sum + c.getBoundingClientRect().width, 0);
      // Padding, border and gaps don't shrink; everything else does.
      const fixed = width - scaled;
      const natural = scaled / fitRef.current;
      // The window is capped a little inside the screen's width.
      const room = window.screen.width - 16 - fixed;
      const next = Math.min(1, room / natural);
      if (Math.abs(next - fitRef.current) > 0.004) {
        fitRef.current = next;
        setFit(next);
      }
      invoke("dock_set_bar_width", { width: Math.ceil(fixed + natural * next) });
    };
    const ro = new ResizeObserver(report);
    ro.observe(el);
    const mo = new MutationObserver(report);
    mo.observe(el, { childList: true, subtree: true });
    report();
    return () => {
      ro.disconnect();
      mo.disconnect();
    };
  }, []);
  const draggedPin = useRef<string | null>(null);
  const now = useClock();
  const apps = useApps();
  const updates = useUpdateCheck(settings?.updates);

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
        await CheckMenuItem.new({
          text: "Show desktop button",
          checked: settings.peek_button !== false,
          action: () => updateSettings({ peek_button: settings.peek_button === false }).catch(console.error),
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
        className={`bar${dragging ? " dragging" : ""}${editing ? " editing" : ""}`}
        style={fit < 1 ? ({ "--fit": fit } as React.CSSProperties) : undefined}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
      >
        {settings?.style === "liquid" && <LiquidGlass radius={8} />}
        {(leftWidgets.length > 0 || editing) && (
          <>
            <DockWidgets widgets={leftWidgets} all={allWidgets} side="left" edit={editing} />
            <span className="divider" />
          </>
        )}
        <StartButton />
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
        {(rightWidgets.length > 0 || editing) && (
          <>
            <span className="divider" />
            <DockWidgets widgets={rightWidgets} all={allWidgets} side="right" edit={editing} />
          </>
        )}
        <span className="divider" />
        {updates.update && settings?.updates !== "off" && (
          <button
            className="update-pill"
            title={`luma-ai-bar ${updates.update.version} is ready. Click to install it; the dock restarts.`}
            disabled={updates.installing}
            onClick={() => updates.install()}
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d="M12 4v11m0 0-4.5-4.5M12 15l4.5-4.5M5 20h14" />
            </svg>
            {updates.installing ? (updates.percent == null ? "Updating" : `${updates.percent}%`) : "Update"}
          </button>
        )}
        <Clock now={now} />
        {settings?.peek_button !== false && (
          // Like the Windows taskbar: a sliver at the far end that shows the desktop.
          <button className="peek" aria-label="Show desktop" title="Show desktop" onClick={() => invoke("show_desktop")} />
        )}
      </div>
    </div>
  );
}
