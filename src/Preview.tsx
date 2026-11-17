import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AppEntry, WindowInfo } from "./apps";
import { useSettings } from "./settings";
import "./preview.css";

type PreviewData = { key: string; name: string; icon: string; windows: WindowInfo[] };

/** The hover popup: one card per window, live thumbnails drawn by DWM into `.thumb`. */
export default function Preview() {
  const [data, setData] = useState<PreviewData | null>(null);
  const [icon, setIcon] = useState<string | null>(null);
  const root = useRef<HTMLDivElement>(null);
  useSettings(); // theme

  useEffect(() => {
    const a = listen<PreviewData>("preview-data", (e) => setData(e.payload));
    // Keep the cards in step with windows opening, closing or being renamed.
    const b = listen<AppEntry[]>("apps-changed", (e) =>
      setData((d) => {
        if (!d) return d;
        const app = e.payload.find((x) => x.key === d.key);
        if (!app || app.windows.length === 0) {
          invoke("preview_hide");
          return null;
        }
        return { ...d, windows: app.windows };
      }),
    );
    return () => {
      a.then((f) => f());
      b.then((f) => f());
    };
  }, []);

  useEffect(() => {
    if (!data) return;
    invoke<string | null>("app_icon", { path: data.icon }).then(setIcon).catch(() => setIcon(null));
  }, [data?.icon]);

  // After every render, tell Rust where each live thumbnail goes.
  useLayoutEffect(() => {
    if (!data || !root.current) return;
    const rects = Array.from(root.current.querySelectorAll<HTMLElement>(".thumb[data-live]")).map((el) => {
      const r = el.getBoundingClientRect();
      return { hwnd: Number(el.dataset.hwnd), x: r.left, y: r.top, w: r.width, h: r.height };
    });
    invoke("preview_layout", { rects }).catch(console.error);
  }, [data]);

  if (!data) return null;

  return (
    <div
      ref={root}
      className="preview"
      onMouseEnter={() => invoke("preview_hover", { source: "preview", inside: true })}
      onMouseLeave={() => invoke("preview_hover", { source: "preview", inside: false })}
    >
      {data.windows.map((w) => (
        <div
          key={w.hwnd}
          className={`card${w.attention ? " attention" : ""}`}
          onClick={() => {
            invoke("window_focus", { hwndValue: w.hwnd });
            invoke("preview_hide");
          }}
        >
          <div className="head">
            {icon && <img src={icon} alt="" />}
            <span className="title">{w.title}</span>
            <button
              className="close"
              title="Close window"
              onClick={(e) => {
                e.stopPropagation();
                invoke("window_close", { hwndValue: w.hwnd });
              }}
            >
              ×
            </button>
          </div>
          {w.minimized ? (
            <div className="thumb minimized">
              {icon && <img src={icon} alt="" />}
              <span>Minimized</span>
            </div>
          ) : (
            <div className="thumb" data-live data-hwnd={w.hwnd} />
          )}
        </div>
      ))}
    </div>
  );
}
