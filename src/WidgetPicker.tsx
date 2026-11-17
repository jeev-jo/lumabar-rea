import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSettings } from "./settings";
import { byKind, GROUPS, SIZE_LABEL, WIDGETS, type WidgetKind } from "./widgets/registry";
import type { WidgetInstance, WidgetSize } from "./widgets/shared";
import "./widgets/widgets.css";
import "./settings-page.css";
import "./picker.css";

type Category = "all" | "dock" | (typeof GROUPS)[number];

// Segoe Fluent Icons glyphs for the sidebar.
const GLYPH: Record<Category, string> = {
  all: "",
  dock: "",
  AI: "",
  Clocks: "",
  Productivity: "",
  System: "",
  Weather: "",
};

const TITLE: Record<Category, string> = {
  all: "All widgets",
  dock: "In your dock",
  AI: "AI",
  Clocks: "Clocks",
  Productivity: "Productivity",
  System: "System",
  Weather: "Weather",
};

function Tile({ instance, preview }: { instance: WidgetInstance; preview?: boolean }) {
  const k = byKind(instance.kind);
  if (!k) return null;
  const T = k.Tile;
  return (
    <div className={`widget size-${instance.size}`}>
      <T instance={instance} preview={preview} />
    </div>
  );
}

function Sizes({ kind, value, onChange }: { kind: WidgetKind; value: WidgetSize; onChange: (s: WidgetSize) => void }) {
  return (
    <div className="chips" role="radiogroup" aria-label="Size">
      {kind.sizes.map((s) => (
        <button key={s} role="radio" aria-checked={s === value} className={s === value ? "on" : ""} onClick={() => onChange(s)} title={SIZE_LABEL[s]}>
          {SIZE_LABEL[s][0]}
        </button>
      ))}
    </div>
  );
}

/** The widgets that are in the dock, as live tiles: click to edit, drag to reorder. */
function DockStrip({ widgets, selected, onSelect }: { widgets: WidgetInstance[]; selected: string | null; onSelect: (id: string | null) => void }) {
  const dragged = useRef<string | null>(null);
  const [over, setOver] = useState<string | null>(null);
  const reorder = (ids: string[]) => invoke("widgets_reorder", { ids });
  const sel = widgets.find((w) => w.id === selected);
  const selKind = sel && byKind(sel.kind);
  const i = sel ? widgets.indexOf(sel) : -1;
  const move = (d: number) => {
    const ids = widgets.map((w) => w.id);
    const j = i + d;
    if (i < 0 || j < 0 || j >= ids.length) return;
    [ids[i], ids[j]] = [ids[j], ids[i]];
    reorder(ids);
  };

  if (!widgets.length) {
    return <div className="strip-empty">Nothing in the dock yet. Add a widget below and it appears in the dock straight away.</div>;
  }
  return (
    <div className="dock-strip">
      <div className="strip-tiles">
        {widgets.map((w) => (
          <div
            key={w.id}
            className={`strip-item${selected === w.id ? " selected" : ""}${over === w.id ? " over" : ""}`}
            draggable
            onClick={() => onSelect(selected === w.id ? null : w.id)}
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
              setOver(null);
              const from = dragged.current;
              dragged.current = null;
              if (!from || from === w.id) return;
              const ids = widgets.map((x) => x.id).filter((x) => x !== from);
              ids.splice(ids.indexOf(w.id), 0, from);
              reorder(ids);
            }}
            title={`${byKind(w.kind)?.name ?? w.kind} — click to edit, drag to reorder`}
          >
            <Tile instance={w} />
            <button
              className="remove-badge"
              title="Remove from dock"
              onClick={(e) => {
                e.stopPropagation();
                if (selected === w.id) onSelect(null);
                invoke("widgets_remove", { id: w.id });
              }}
            >
              {""}
            </button>
          </div>
        ))}
      </div>
      {sel && selKind && (
        <div className="strip-toolbar">
          <b>{selKind.name}</b>
          <Sizes kind={selKind} value={sel.size} onChange={(size) => invoke("widget_update", { id: sel.id, size })} />
          <span className="sep" />
          <button className="icon" title="Move left" disabled={i === 0} onClick={() => move(-1)}>
            {""}
          </button>
          <button className="icon" title="Move right" disabled={i === widgets.length - 1} onClick={() => move(1)}>
            {""}
          </button>
          <span className="sep" />
          <button
            className="danger"
            onClick={() => {
              onSelect(null);
              invoke("widgets_remove", { id: sel.id });
            }}
          >
            Remove
          </button>
        </div>
      )}
    </div>
  );
}

function Card({ kind, inDock }: { kind: WidgetKind; inDock: number }) {
  const [size, setSize] = useState<WidgetSize>(kind.defaultSize);
  const [added, setAdded] = useState(false);
  useEffect(() => {
    if (!added) return;
    const t = setTimeout(() => setAdded(false), 1400);
    return () => clearTimeout(t);
  }, [added]);
  return (
    <div className="pcard">
      <div className="stage">
        <div className="bar-sample">
          <Tile instance={{ id: `preview-${kind.kind}`, kind: kind.kind, size, config: {} }} preview />
        </div>
      </div>
      <div className="pcard-body">
        <div className="pcard-title">
          <b>{kind.name}</b>
          {inDock > 0 && <span className="pill">{inDock} in dock</span>}
        </div>
        <p>{kind.description}</p>
        <div className="pcard-actions">
          <Sizes kind={kind} value={size} onChange={setSize} />
          <button
            className={`add${added ? " done" : ""}`}
            onClick={() => invoke("widgets_add", { kind: kind.kind, size }).then(() => setAdded(true))}
          >
            {added ? " Added" : " Add"}
          </button>
        </div>
      </div>
    </div>
  );
}

/** Widget picker: browse by category or search, add with one click, arrange what's in the dock. */
export default function WidgetPicker() {
  const settings = useSettings();
  const [cat, setCat] = useState<Category>("all");
  const [q, setQ] = useState("");
  const [selected, setSelected] = useState<string | null>(null);

  useEffect(() => {
    document.documentElement.classList.add("settings-page", "picker-page");
  }, []);

  const query = q.trim().toLowerCase();
  const matches = useMemo(
    () => WIDGETS.filter((k) => !query || `${k.name} ${k.description} ${k.group}`.toLowerCase().includes(query)),
    [query],
  );

  if (!settings) return null;
  const inDock = settings.widgets;
  const countIn = (kind: string) => inDock.filter((w) => w.kind === kind).length;
  const groupsShown = (cat === "all" || cat === "dock" ? GROUPS : GROUPS.filter((g) => g === cat)).filter((g) =>
    matches.some((k) => k.group === g),
  );
  const shownCount = groupsShown.reduce((n, g) => n + matches.filter((k) => k.group === g).length, 0);

  const nav: Category[] = ["all", "dock", ...GROUPS];
  const navCount = (c: Category) =>
    c === "all" ? WIDGETS.length : c === "dock" ? inDock.length : WIDGETS.filter((k) => k.group === c).length;

  return (
    <div className="picker2">
      <aside>
        <label className="search">
          <span className="glyph">{""}</span>
          <input
            placeholder="Search widgets"
            value={q}
            onChange={(e) => {
              setQ(e.target.value);
              if (cat === "dock") setCat("all");
            }}
          />
        </label>
        <nav>
          {nav.map((c) => (
            <button key={c} className={cat === c ? "on" : ""} onClick={() => setCat(c)}>
              <span className="glyph">{GLYPH[c]}</span>
              <span className="label">{TITLE[c]}</span>
              <span className="count">{navCount(c)}</span>
            </button>
          ))}
        </nav>
      </aside>

      <main>
        <header>
          <h1>{query ? `Results for “${q.trim()}”` : TITLE[cat]}</h1>
          {cat !== "dock" && <span className="pill">{shownCount}</span>}
          <span className="sub">{inDock.length} in dock</span>
        </header>

        {(cat === "all" || cat === "dock") && !query && (
          <section>
            {cat === "all" && <h2>In your dock</h2>}
            <DockStrip widgets={inDock} selected={selected} onSelect={setSelected} />
          </section>
        )}

        {cat !== "dock" &&
          groupsShown.map((g) => (
            <section key={g}>
              {(cat === "all" || query) && (
                <h2>
                  <span className="glyph">{GLYPH[g]}</span> {g}
                </h2>
              )}
              <div className="pgrid">
                {matches
                  .filter((k) => k.group === g)
                  .map((k) => (
                    <Card key={k.kind} kind={k} inDock={countIn(k.kind)} />
                  ))}
              </div>
            </section>
          ))}

        {cat !== "dock" && shownCount === 0 && <p className="none">No widgets match “{q.trim()}”.</p>}
      </main>
    </div>
  );
}
