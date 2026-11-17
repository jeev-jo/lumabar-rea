import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSettings } from "./settings";
import { byKind, GROUPS, SIZE_LABEL, WIDGETS } from "./widgets/registry";
import type { WidgetInstance, WidgetSize } from "./widgets/shared";
import "./widgets/widgets.css";
import "./settings-page.css";

function Preview({ kind, size }: { kind: string; size: WidgetSize }) {
  const k = byKind(kind)!;
  const Tile = k.Tile;
  return (
    <div className="tile-preview">
      <div className={`widget size-${size}`}>
        <Tile instance={{ id: `preview-${kind}`, kind, size, config: {} }} preview />
      </div>
    </div>
  );
}

function SizeChoice({ sizes, value, onChange }: { sizes: WidgetSize[]; value: WidgetSize; onChange: (s: WidgetSize) => void }) {
  return (
    <div className="segmented">
      {sizes.map((s) => (
        <button key={s} className={s === value ? "on" : ""} onClick={() => onChange(s)}>
          {SIZE_LABEL[s]}
        </button>
      ))}
    </div>
  );
}

/** Widget picker: what's in the dock (reorder, resize, remove) and everything you can add. */
export default function WidgetPicker() {
  const settings = useSettings();
  const [sizes, setSizes] = useState<Record<string, WidgetSize>>({});
  const dragged = useRef<string | null>(null);
  const [over, setOver] = useState<string | null>(null);

  useEffect(() => {
    document.documentElement.classList.add("settings-page");
  }, []);

  if (!settings) return null;
  const inDock = settings.widgets;
  const reorder = (ids: string[]) => invoke("widgets_reorder", { ids });
  const move = (i: number, d: number) => {
    const ids = inDock.map((w) => w.id);
    const j = i + d;
    if (j < 0 || j >= ids.length) return;
    [ids[i], ids[j]] = [ids[j], ids[i]];
    reorder(ids);
  };
  const drop = (target: WidgetInstance) => {
    const from = dragged.current;
    dragged.current = null;
    setOver(null);
    if (!from || from === target.id) return;
    const ids = inDock.map((w) => w.id).filter((x) => x !== from);
    ids.splice(ids.indexOf(target.id), 0, from);
    reorder(ids);
  };

  return (
    <main className="picker">
      <header>
        <h1>Widgets</h1>
      </header>

      <section>
        <h2>In your dock</h2>
        {inDock.length === 0 && <p className="fine">No widgets yet. Add one below — it appears in the dock straight away.</p>}
        <ul className="in-dock">
          {inDock.map((w, i) => {
            const k = byKind(w.kind);
            if (!k) return null;
            return (
              <li
                key={w.id}
                className={over === w.id ? "over" : ""}
                draggable
                onDragStart={(e) => {
                  dragged.current = w.id;
                  e.dataTransfer.effectAllowed = "move";
                  e.dataTransfer.setData("text/plain", w.id);
                }}
                onDragOver={(e) => {
                  e.preventDefault();
                  setOver(w.id);
                }}
                onDragLeave={() => setOver((o) => (o === w.id ? null : o))}
                onDrop={(e) => {
                  e.preventDefault();
                  drop(w);
                }}
              >
                <span className="grip" title="Drag to reorder">
                  ⋮⋮
                </span>
                <span className="name">{k.name}</span>
                <SizeChoice sizes={k.sizes} value={w.size} onChange={(size) => invoke("widget_update", { id: w.id, size })} />
                <span className="order">
                  <button className="glyph" title="Move left in the dock" disabled={i === 0} onClick={() => move(i, -1)}>
                    {""}
                  </button>
                  <button className="glyph" title="Move right in the dock" disabled={i === inDock.length - 1} onClick={() => move(i, 1)}>
                    {""}
                  </button>
                </span>
                <button className="action" onClick={() => invoke("widgets_remove", { id: w.id })}>
                  Remove
                </button>
              </li>
            );
          })}
        </ul>
      </section>

      {GROUPS.map((group) => (
        <section key={group}>
          <h2>{group}</h2>
          <div className="gallery">
            {WIDGETS.filter((k) => k.group === group).map((k) => {
              const size = sizes[k.kind] ?? k.defaultSize;
              return (
                <div key={k.kind} className="card">
                  <Preview kind={k.kind} size={size} />
                  <div className="meta">
                    <b>{k.name}</b>
                    <span>{k.description}</span>
                  </div>
                  <div className="add-row">
                    <SizeChoice sizes={k.sizes} value={size} onChange={(s) => setSizes({ ...sizes, [k.kind]: s })} />
                    <button className="action primary" onClick={() => invoke("widgets_add", { kind: k.kind, size })}>
                      Add
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        </section>
      ))}
    </main>
  );
}
