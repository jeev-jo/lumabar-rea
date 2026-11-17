import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";

export type WindowInfo = {
  hwnd: number;
  title: string;
  minimized: boolean;
  attention: boolean;
};

export type AppEntry = {
  key: string;
  name: string;
  pinned: boolean;
  launch: string;
  icon: string;
  windows: WindowInfo[];
  active: boolean;
  attention: boolean;
};

export function useApps() {
  const [apps, setApps] = useState<AppEntry[]>([]);
  useEffect(() => {
    invoke<AppEntry[]>("apps_snapshot").then(setApps).catch(console.error);
    const unlisten = listen<AppEntry[]>("apps-changed", (e) => setApps(e.payload));
    return () => {
      unlisten.then((f) => f());
    };
  }, []);
  return apps;
}

const iconCache = new Map<string, Promise<string | null>>();

function useIcon(path: string) {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    let p = iconCache.get(path);
    if (!p) {
      p = invoke<string | null>("app_icon", { path }).catch(() => null);
      iconCache.set(path, p);
    }
    p.then((s) => live && setSrc(s));
    return () => {
      live = false;
    };
  }, [path]);
  return src;
}

const short = (s: string, n = 48) => (s.length > n ? s.slice(0, n - 1) + "…" : s);

async function showAppMenu(app: AppEntry) {
  const items = [];
  for (const w of app.windows) {
    items.push(
      await MenuItem.new({
        text: short(w.title) + (w.minimized ? "  (minimized)" : ""),
        action: () => invoke("window_focus", { hwndValue: w.hwnd }),
      }),
    );
  }
  if (items.length) items.push(await PredefinedMenuItem.new({ item: "Separator" }));
  items.push(
    await MenuItem.new({
      text: app.windows.length ? `New ${app.name} window` : `Open ${app.name}`,
      action: () => invoke("app_launch", { key: app.key }),
    }),
    await MenuItem.new({
      text: app.pinned ? "Unpin from dock" : "Pin to dock",
      action: () => invoke(app.pinned ? "app_unpin" : "app_pin", { key: app.key }),
    }),
  );
  if (app.windows.length) {
    items.push(
      await MenuItem.new({
        text: app.windows.length === 1 ? "Close window" : `Close all ${app.windows.length} windows`,
        action: () => invoke("app_close_all", { key: app.key }),
      }),
    );
  }
  const menu = await Menu.new({ items });
  await menu.popup();
}

type Props = {
  app: AppEntry;
  onDragStartPin: (key: string) => void;
  onDropOnPin: (key: string) => void;
};

export function AppButton({ app, onDragStartPin, onDropOnPin }: Props) {
  const icon = useIcon(app.icon);
  const running = app.windows.length > 0;
  const label =
    app.windows.length > 1 ? `${app.name} — ${app.windows.length} windows` : app.windows[0]?.title || app.name;

  const classes = ["app"];
  if (running) classes.push("running");
  if (app.active) classes.push("active");
  if (app.attention) classes.push("attention");

  return (
    <button
      className={classes.join(" ")}
      title={label}
      draggable={app.pinned}
      onClick={() => invoke("app_activate", { key: app.key }).catch(console.error)}
      // Middle click starts a new instance, like the Windows taskbar.
      onMouseDown={(e) => e.button === 1 && e.preventDefault()}
      onAuxClick={(e) => {
        if (e.button === 1) invoke("app_launch", { key: app.key }).catch(console.error);
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        showAppMenu(app).catch(console.error);
      }}
      onDragStart={(e) => {
        e.dataTransfer.effectAllowed = "move";
        e.dataTransfer.setData("text/plain", app.key);
        onDragStartPin(app.key);
      }}
      onDragOver={(e) => {
        if (app.pinned) e.preventDefault();
      }}
      onDrop={(e) => {
        e.preventDefault();
        if (app.pinned) onDropOnPin(app.key);
      }}
    >
      {icon ? <img src={icon} alt="" draggable={false} /> : <span className="fallback">{app.name.slice(0, 1)}</span>}
      <span className="dots">
        {running &&
          app.windows.slice(0, 3).map((w) => <i key={w.hwnd} />)}
      </span>
    </button>
  );
}
