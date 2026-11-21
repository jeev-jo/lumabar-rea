import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSettings } from "./settings";
import { byKind, GROUPS, SIZE_LABEL, WIDGETS, type WidgetKind } from "./widgets/registry";
import type { WidgetInstance, WidgetSide, WidgetSize } from "./widgets/shared";
import "./widgets/widgets.css";
import "./settings-page.css";
import "./picker.css";

type Category = "all" | (typeof GROUPS)[number];

// Segoe Fluent Icons glyphs for the sidebar.
const GLYPH: Record<Category, string> = {
  all: "",
  AI: "",
  Clocks: "",
  Media: "",
  Productivity: "",
  System: "",
  Weather: "",
  Analytics: "",
  "Apps & Files": "",
  Developer: "",
  Finance: "",
  "Online & Mail": "",
  Social: "",
  Stocks: "",
};

const TITLE: Record<Category, string> = {
  all: "All widgets",
  AI: "AI",
  Clocks: "Clocks",
  Media: "Media",
  Productivity: "Productivity",
  System: "System",
  Weather: "Weather",
  Analytics: "Analytics",
  "Apps & Files": "Apps & Files",
  Developer: "Developer",
  Finance: "Finance",
  "Online & Mail": "Online & Mail",
  Social: "Social",
  Stocks: "Stocks",
};

function Tile({ instance, preview }: { instance: WidgetInstance; preview?: boolean }) {
  const k = byKind(instance.kind);
  if (!k) return null;
  const T = k.Tile;
  return (
    <div className={`widget size-${instance.size} kind-${instance.kind}`}>
      <T instance={instance} preview={preview} />
    </div>
  );
}

function Sizes({ kind, value, onChange }: { kind: WidgetKind; value: WidgetSize; onChange: (s: WidgetSize) => void }) {
  return (
    <div className="sizes" role="radiogroup" aria-label="Size">
      {kind.sizes.map((s) => (
        <button key={s} role="radio" aria-checked={s === value} className={s === value ? "on" : ""} onClick={() => onChange(s)} title={SIZE_LABEL[s]}>
          {SIZE_LABEL[s][0]}
        </button>
      ))}
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
    <div className={`pcard${inDock > 0 ? " in-dock" : ""}`}>
      <div className="stage">
        <div className="bar-sample">
          <Tile instance={{ id: `preview-${kind.kind}`, kind: kind.kind, size, config: {} }} preview />
        </div>
        {inDock > 0 && (
          <span className="in-dock-badge" title={`${inDock} in the dock`}>
            <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d="M5 12.5l4.5 4.5L19 7.5" />
            </svg>
            {inDock > 1 ? `${inDock} in dock` : "In dock"}
          </span>
        )}
      </div>
      <div className="pcard-body">
        <b className="pcard-title">{kind.name}</b>
        <p>{kind.description}</p>
        <div className="pcard-actions">
          <Sizes kind={kind} value={size} onChange={setSize} />
          <button
            className={`add${added ? " done" : ""}`}
            onClick={() => invoke("widgets_add", { kind: kind.kind, size, side }).then(() => setAdded(true))}
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d={added ? "M5 12.5l4.5 4.5L19 7.5" : "M12 5v14M5 12h14"} />
            </svg>
            {added ? "Added" : "Add"}
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
  const [addSide, setAddSide] = useState<WidgetSide>("right");

  useEffect(() => {
    document.documentElement.classList.add("settings-page", "picker-page");
  }, []);

  const query = q.trim().toLowerCase();
  const matches = useMemo(
    () => WIDGETS.filter((k) => !k.hidden && (!query || `${k.name} ${k.description} ${k.group}`.toLowerCase().includes(query))),
    [query],
  );

  if (!settings) return null;
  const inDock = settings.widgets;
  const countIn = (kind: string) => inDock.filter((w) => w.kind === kind).length;
  // A search looks through every group, whichever one is open.
  const groupsShown = (cat === "all" || query ? GROUPS : GROUPS.filter((g) => g === cat)).filter((g) =>
    matches.some((k) => k.group === g),
  );
  const shownCount = groupsShown.reduce((n, g) => n + matches.filter((k) => k.group === g).length, 0);

  const nav: Category[] = ["all", ...GROUPS];
  const offered = WIDGETS.filter((k) => !k.hidden);
  const navCount = (c: Category) => (c === "all" ? offered.length : offered.filter((k) => k.group === c).length);

  return (
    <div className="picker2">
      <aside>
        <div className="brand">
          <span className="logo glyph" aria-hidden="true">{GLYPH.all}</span>
          <div>
            <div className="name">Widgets</div>
            <div className="version">{offered.length} available</div>
          </div>
        </div>
        <label className="search">
          <span className="glyph">{""}</span>
          <input
            placeholder="Search widgets"
            value={q}
            onChange={(e) => {
              setQ(e.target.value);
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
          <div className="heading">
            <h1>{query ? `Results for “${q.trim()}”` : TITLE[cat]}</h1>
            <p>
              {shownCount} widget{shownCount === 1 ? "" : "s"} · {inDock.length} in your dock
            </p>
          </div>
          <div className="add-side">
            <span>Add to the</span>
            <div className="segmented" role="radiogroup" aria-label="Add new widgets to">
              {(["left", "right"] as WidgetSide[]).map((side) => (
                <button key={side} role="radio" aria-checked={addSide === side} className={addSide === side ? "on" : ""} onClick={() => setAddSide(side)}>
                  {side === "left" ? "Left" : "Right"}
                </button>
              ))}
            </div>
          </div>
        </header>

        <div className="edit-banner">
          <span className="glyph">{""}</span>
          <div>
            <b>The dock is in arrange mode</b>
            <span>
              Drag widgets on the dock to reorder them, or across the apps to switch sides. Use the red badge to remove one, and
              right-click a widget to change its size.
            </span>
          </div>
          <button className="action primary" onClick={() => invoke("close_widget_picker")}>
            Done
          </button>
        </div>

        {groupsShown.map((g) => (
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

        {shownCount === 0 && (
          <div className="none">
            <b>No widgets match “{q.trim()}”</b>
            <span>Try another word, such as a service or app name.</span>
          </div>
        )}
      </main>
    </div>
  );
}
