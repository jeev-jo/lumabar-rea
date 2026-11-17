import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSettings } from "./settings";
import { byKind, GROUPS, SIZE_LABEL, WIDGETS, type WidgetKind } from "./widgets/registry";
import { sideOf, type WidgetInstance, type WidgetSide, type WidgetSize } from "./widgets/shared";
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

/**
 * The widgets in the dock as live tiles, in two zones: left and right of the
 * app icons. Click a tile to edit it; drag it within a zone or across to the
 * other side.
 */
function DockStrip({ widgets, selected, onSelect }: { widgets: WidgetInstance[]; selected: string | null; onSelect: (id: string | null) => void }) {
  const dragged = useRef<string | null>(null);
  const [over, setOver] = useState<string | null>(null);
  const sel = widgets.find((w) => w.id === selected);
  const selKind = sel && byKind(sel.kind);
  const zone = (side: WidgetSide) => widgets.filter((w) => sideOf(w) === side);

  /** Put `id` on `side`, just before `beforeId` (or at the end of that side). */
  const place = (id: string, side: WidgetSide, beforeId?: string) => {
    const moving = widgets.find((w) => w.id === id);
    if (!moving) return;
    const rest = widgets.filter((w) => w.id !== id);
    const order = [...rest.filter((w) => sideOf(w) === "left"), ...rest.filter((w) => sideOf(w) === "right")].map((w) => w.id);
    let at: number;
    if (beforeId && beforeId !== id) {
      at = order.indexOf(beforeId);
    } else {
      // End of the chosen side.
      const lastOfSide = [...rest].reverse().find((w) => sideOf(w) === side);
      at = lastOfSide ? order.indexOf(lastOfSide.id) + 1 : side === "left" ? 0 : order.length;
    }
    order.splice(at, 0, id);
    if (sideOf(moving) !== side) invoke("widget_update", { id, side });
    invoke("widgets_reorder", { ids: order });
  };

  const move = (d: number) => {
    if (!sel) return;
    const list = zone(sideOf(sel));
    const i = list.indexOf(sel);
    const j = i + d;
    if (j < 0 || j >= list.length) return;
    // Moving right = placing it before the item after its neighbour.
    place(sel.id, sideOf(sel), d < 0 ? list[j].id : list[j + 1]?.id);
  };

  const zoneView = (side: WidgetSide) => {
    const list = zone(side);
    return (
      <div
        key={side}
        className={`pz${over === `zone-${side}` ? " over" : ""}`}
        onDragOver={(e) => {
          e.preventDefault();
          if (!over || over.startsWith("zone")) setOver(`zone-${side}`);
        }}
        onDragLeave={(e) => {
          if (!e.currentTarget.contains(e.relatedTarget as Node)) setOver(null);
        }}
        onDrop={(e) => {
          e.preventDefault();
          setOver(null);
          const from = dragged.current;
          dragged.current = null;
          if (from) place(from, side);
        }}
      >
        <div className="pz-label">
          <span className="glyph">{side === "left" ? "\uE76B" : "\uE76C"}</span>
          {side === "left" ? "Left of your apps" : "Right of your apps"}
        </div>
        <div className="strip-tiles">
          {list.length === 0 && <div className="pz-empty">Drop a widget here</div>}
          {list.map((w) => (
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
                e.stopPropagation();
                setOver(w.id);
              }}
              onDrop={(e) => {
                e.preventDefault();
                e.stopPropagation();
                setOver(null);
                const from = dragged.current;
                dragged.current = null;
                if (from && from !== w.id) place(from, side, w.id);
              }}
              title={`${byKind(w.kind)?.name ?? w.kind} — click to edit, drag to move`}
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
                {"\uE738"}
              </button>
            </div>
          ))}
        </div>
      </div>
    );
  };

  if (!widgets.length) {
    return <div className="strip-empty">Nothing in the dock yet. Add a widget below and it appears in the dock straight away.</div>;
  }
  const selList = sel ? zone(sideOf(sel)) : [];
  const selIndex = sel ? selList.indexOf(sel) : -1;
  return (
    <div className="dock-strip">
      <div className="pz-row">
        {zoneView("left")}
        <div className="pz-apps" title="Your app icons sit here">
          <span className="glyph">{"\uE8A9"}</span>
          Apps
        </div>
        {zoneView("right")}
      </div>
      {sel && selKind && (
        <div className="strip-toolbar">
          <b>{selKind.name}</b>
          <Sizes kind={selKind} value={sel.size} onChange={(size) => invoke("widget_update", { id: sel.id, size })} />
          <span className="sep" />
          <div className="chips wide" role="radiogroup" aria-label="Side">
            {(["left", "right"] as WidgetSide[]).map((side) => (
              <button key={side} role="radio" aria-checked={sideOf(sel) === side} className={sideOf(sel) === side ? "on" : ""} onClick={() => place(sel.id, side)}>
                {side === "left" ? "Left" : "Right"}
              </button>
            ))}
          </div>
          <span className="sep" />
          <button className="icon" title="Move left" disabled={selIndex <= 0} onClick={() => move(-1)}>
            {"\uE76B"}
          </button>
          <button className="icon" title="Move right" disabled={selIndex === selList.length - 1} onClick={() => move(1)}>
            {"\uE76C"}
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

function Card({ kind, inDock, side }: { kind: WidgetKind; inDock: number; side: WidgetSide }) {
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
            onClick={() => invoke("widgets_add", { kind: kind.kind, size, side }).then(() => setAdded(true))}
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
  const [addSide, setAddSide] = useState<WidgetSide>("right");

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
          <span className="spacer" />
          <span className="sub">Add new widgets to the</span>
          <div className="chips wide" role="radiogroup" aria-label="Add new widgets to">
            {(["left", "right"] as WidgetSide[]).map((side) => (
              <button key={side} role="radio" aria-checked={addSide === side} className={addSide === side ? "on" : ""} onClick={() => setAddSide(side)}>
                {side === "left" ? "Left" : "Right"}
              </button>
            ))}
          </div>
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
                    <Card key={k.kind} kind={k} inDock={countIn(k.kind)} side={addSide} />
                  ))}
              </div>
            </section>
          ))}

        {cat !== "dock" && shownCount === 0 && <p className="none">No widgets match “{q.trim()}”.</p>}
      </main>
    </div>
  );
}
