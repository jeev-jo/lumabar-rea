/**
 * Online service widgets: revenue, stocks and crypto, exchange rates,
 * analytics, developer tools, GitHub, LeetCode, mail, social and calendar,
 * plus more AI plan limits. Rust does every request (see online.rs); keys are
 * pasted here, encrypted on this PC, and never come back to the UI.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { KeyForm, MeterBar, ServiceIcon, time, until } from "./ai";
import { useSettings } from "../settings";
import { updateConfig, useWidgetData, type WidgetProps } from "./shared";

type Meter = { label: string; percent: number; resets_at: string | null };
type Item = { title: string; detail: string; value: string; url: string | null };
export type Reading = {
  provider: string;
  headline: string;
  caption: string;
  lines: [string, string][];
  meters: Meter[];
  data_as_of: string | null;
  fetched_at: number;
  series?: number[];
  series_labels?: string[];
  value?: number | null;
  currency?: string | null;
  items?: Item[];
};
type Result = { reading: Reading | null; error: string | null; stale: boolean };

type Field = { key: string; label: string; placeholder?: string; number?: boolean };

type View = "value" | "chart" | "change" | "list" | "calendar" | "grid" | "meters";

export type ServiceDef = {
  provider: string;
  short: string;
  name: string;
  icon: string;
  view: View;
  /** Pasted secret: what it is and where to get it. Omit for services that need none. */
  keyHint?: string;
  keyLabel?: string;
  /** Services read from something on this PC, or public data: where it comes from. */
  source?: string;
  /** Settings shown in the details view, saved with the widget. */
  fields?: Field[];
  /** Refresh interval in ms (default one minute). */
  every?: number;
  demo: Partial<Reading>;
};

const glyph = {
  mail: "",
  calendar: "",
  currency: "",
  up: "",
  down: "",
  refresh: "",
};

const openLink = (url: string) => invoke("open_link", { url }).catch(console.error);

function demoResult(def: ServiceDef): Result {
  return {
    reading: {
      provider: def.provider,
      headline: "",
      caption: def.name,
      lines: [],
      meters: [],
      data_as_of: null,
      fetched_at: Date.now(),
      ...def.demo,
    },
    error: null,
    stale: false,
  };
}

/** The last result, shared by the tile and the details view; the tile refreshes it. */
function useReading(id: string, def: ServiceDef, config: Record<string, unknown>, preview?: boolean) {
  const [result, save] = useWidgetData<Result | null>(id, preview ? demoResult(def) : null, preview);
  const [busy, setBusy] = useState(false);
  const cfg = JSON.stringify(config ?? {});
  const refresh = useCallback(
    async (manual = false) => {
      if (preview) return;
      setBusy(true);
      try {
        save(await invoke<Result>("ai_usage_fetch", { id, provider: def.provider, manual, config: JSON.parse(cfg) }));
      } finally {
        setBusy(false);
      }
    },
    [id, def.provider, cfg, preview, save],
  );
  return { result, refresh, busy, cfg };
}

// ---- small charts ----------------------------------------------------------------

function Spark({ values, up }: { values: number[]; up?: boolean }) {
  if (values.length < 2) return null;
  const w = 100;
  const h = 30;
  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min || 1;
  const pts = values.map((v, i) => `${((i / (values.length - 1)) * w).toFixed(2)},${(h - 2 - ((v - min) / span) * (h - 4)).toFixed(2)}`);
  return (
    <svg className={`spark${up === undefined ? "" : up ? " up" : " down"}`} viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" aria-hidden>
      <polyline points={pts.join(" ")} fill="none" strokeWidth="2" vectorEffect="non-scaling-stroke" strokeLinejoin="round" strokeLinecap="round" />
    </svg>
  );
}

/** A labelled line for the details view, with the value under the pointer. */
function SeriesChart({ values, labels, format, zero = true }: { values: number[]; labels?: string[]; format: (v: number) => string; zero?: boolean }) {
  const [hover, setHover] = useState<number | null>(null);
  if (values.length < 2) return null;
  const W = 300;
  const H = 90;
  const pad = 6;
  // Amounts start from zero; prices and rates use their own range.
  const max = zero ? Math.max(...values, 0) : Math.max(...values);
  const min = zero ? Math.min(...values, 0) : Math.min(...values);
  const span = max - min || 1;
  const x = (i: number) => pad + (i / (values.length - 1)) * (W - pad * 2);
  const y = (v: number) => pad + (1 - (v - min) / span) * (H - pad * 2);
  const line = values.map((v, i) => `${x(i)},${y(v)}`).join(" ");
  const i = hover ?? values.length - 1;
  return (
    <div className="series-chart">
      <div className="series-head">
        <span>{labels?.[i] ?? (hover === null ? "Latest" : `Point ${i + 1}`)}</span>
        <b>{format(values[i])}</b>
      </div>
      <svg
        viewBox={`0 0 ${W} ${H}`}
        onMouseMove={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          const px = ((e.clientX - r.left) / r.width) * W;
          setHover(Math.max(0, Math.min(values.length - 1, Math.round(((px - pad) / (W - pad * 2)) * (values.length - 1)))));
        }}
        onMouseLeave={() => setHover(null)}
        role="img"
      >
        {zero && <line x1={pad} x2={W - pad} y1={y(0)} y2={y(0)} className="grid" />}
        <polygon points={`${x(0)},${y(zero ? Math.max(min, 0) : min)} ${line} ${x(values.length - 1)},${y(zero ? Math.max(min, 0) : min)}`} className="area" />
        <polyline points={line} className="stroke" />
        <circle cx={x(i)} cy={y(values[i])} r="3.5" className="dot" />
      </svg>
      {labels && labels.length > 1 && (
        <div className="series-axis">
          <span>{labels[0]}</span>
          <span>{labels[labels.length - 1]}</span>
        </div>
      )}
    </div>
  );
}

/** GitHub-style contribution grid: one column per week, Monday at the top. */
function ContributionGrid({ values, labels, weeks }: { values: number[]; labels: string[]; weeks: number }) {
  if (!labels.length) return null;
  // Columns are weeks, Sunday on top (as GitHub's own data runs); the last
  // column ends on the latest day.
  const lastDow = new Date(labels[labels.length - 1] + "T12:00").getDay();
  const cells = (weeks - 1) * 7 + lastDow + 1;
  const start = Math.max(0, values.length - cells);
  const shown = values.slice(start);
  const max = Math.max(1, ...shown);
  const level = (v: number) => (v === 0 ? 0 : v <= max * 0.25 ? 1 : v <= max * 0.5 ? 2 : v <= max * 0.75 ? 3 : 4);
  const pad = cells - shown.length;
  return (
    <div className="contrib" style={{ gridTemplateColumns: `repeat(${weeks}, 1fr)` }}>
      {Array.from({ length: pad }, (_, i) => (
        <i key={`p${i}`} className="pad" />
      ))}
      {shown.map((v, i) => (
        <i key={i} className={`l${level(v)}`} title={`${labels[start + i]}: ${v}`} />
      ))}
    </div>
  );
}

// ---- tile ------------------------------------------------------------------------

function fmtWhen(v: string) {
  if (!v) return "";
  const allDay = !v.includes("T");
  const d = new Date(allDay ? v + "T00:00" : v);
  const today = new Date();
  const sameDay = d.toDateString() === today.toDateString();
  const tomorrow = new Date(today.getTime() + 864e5).toDateString() === d.toDateString();
  const day = sameDay ? "Today" : tomorrow ? "Tomorrow" : d.toLocaleDateString([], { weekday: "short", month: "short", day: "numeric" });
  if (allDay) return day;
  const t = d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  return sameDay ? t : `${day} ${t}`;
}

const isDown = (s: string) => /(^|\s)-\d/.test(s);

function ServiceTile({ instance, preview, def }: WidgetProps & { def: ServiceDef }) {
  const { result, refresh, cfg } = useReading(instance.id, def, instance.config, preview);
  useEffect(() => {
    if (preview) return;
    refresh();
    const t = setInterval(refresh, def.every ?? 60_000);
    return () => clearInterval(t);
  }, [def.provider, preview, cfg]);

  const size = instance.size;
  const r = result?.reading;
  const stale = !!result?.stale;
  const icon = <ServiceIcon service={def.icon} label={def.short} size={size === "s" ? 18 : 20} />;
  const title = r ? `${r.caption}${r.headline ? `: ${r.headline}` : ""}${stale ? `\nLast reading at ${time(r.fetched_at)}` : ""}` : result?.error ?? def.name;
  const cls = `w-svc ${size} v-${def.view}${stale ? " stale" : ""}`;

  if (!r) {
    const needsKey = result?.error === "Add a key to start.";
    const needsSetup = result?.error?.startsWith("Add ");
    return (
      <div className={cls} title={result?.error ?? def.name}>
        {icon}
        <span className="setup">{!result ? "…" : needsKey ? `Add ${def.keyLabel ?? "key"}` : needsSetup ? "Set up" : "Can't read"}</span>
      </div>
    );
  }

  const series = r.series ?? [];
  switch (def.view) {
    case "meters": {
      const shown = r.meters.slice(0, 2);
      if (shown.length > 1 && size !== "s") {
        return (
          <div className={`w-ai multi ${size}${stale ? " stale" : ""}`} title={title}>
            {icon}
            <span className="mrows">
              {shown.map((m) => (
                <span key={m.label} className="mrow">
                  <small>{m.label}</small>
                  <MeterBar m={m} thin />
                  <b>{Math.round(m.percent)}%</b>
                </span>
              ))}
            </span>
          </div>
        );
      }
      break;
    }
    case "change": {
      const down = isDown(r.caption);
      const [sym, chg] = r.caption.split(" ");
      return (
        <div className={cls} title={title}>
          {size !== "s" && <span className="sym">{sym.replace(/(USDT|USDC|FDUSD)$/, "")}</span>}
          <span className="value">
            <b>{r.headline}</b>
            <small className={down ? "neg" : "pos"}>{chg}</small>
          </span>
          {size === "l" && <Spark values={series} up={!down} />}
        </div>
      );
    }
    case "chart":
      return (
        <div className={cls} title={title}>
          {size !== "m" && icon}
          <span className="value">
            <b>{r.headline}</b>
            {size !== "s" && <small>{def.short}</small>}
          </span>
          {size !== "s" && <Spark values={series} />}
        </div>
      );
    case "list": {
      const first = r.items?.[0];
      if (def.provider === "watchlist") {
        const items = (r.items ?? []).slice(0, size === "l" ? 3 : size === "m" ? 2 : 1);
        return (
          <div className={`${cls} quotes`} title={title}>
            {items.map((q) => (
              <span key={q.title} className="q">
                <small>{q.title}</small>
                <b className={isDown(q.detail) ? "neg" : "pos"}>{q.detail}</b>
              </span>
            ))}
          </div>
        );
      }
      return (
        <div className={cls} title={first ? `${title}\n${first.detail ? first.detail + ": " : ""}${first.title}` : title}>
          {icon}
          <b className="count">{r.headline}</b>
          {size === "l" && first && <span className="first">{first.detail || first.title}</span>}
          {size === "m" && <small>{def.short}</small>}
        </div>
      );
    }
    case "calendar": {
      if (!r.headline) {
        return (
          <div className={cls} title={title}>
            <span className="glyph">{glyph.calendar}</span>
            {size !== "s" && <small>No events</small>}
          </div>
        );
      }
      const when = fmtWhen(r.headline);
      return (
        <div className={cls} title={`${r.caption}\n${when}`}>
          {size === "s" ? (
            <b className="when">{when.replace(/^(Today|Tomorrow) /, "")}</b>
          ) : (
            <span className="text">
              <b>{r.caption}</b>
              <small>{when}</small>
            </span>
          )}
        </div>
      );
    }
    case "grid":
      return (
        <div className={cls} title={title}>
          {size === "s" ? (
            <>
              {icon}
              <b>{r.headline}</b>
            </>
          ) : (
            <>
              <ContributionGrid values={series} labels={r.series_labels ?? []} weeks={size === "l" ? 16 : 9} />
              <b className="streak">{r.headline}</b>
            </>
          )}
        </div>
      );
  }
  return (
    <div className={cls} title={title}>
      {icon}
      <span className="value">
        <b>{r.headline}</b>
        {size === "l" && <small>{def.short}</small>}
      </span>
    </div>
  );
}

// ---- details view ------------------------------------------------------------------

function Fields({ instance, def }: { instance: WidgetProps["instance"]; def: ServiceDef }) {
  const [draft, setDraft] = useState<Record<string, string>>(() =>
    Object.fromEntries((def.fields ?? []).map((f) => [f.key, String(instance.config[f.key] ?? "")])),
  );
  const changed = (def.fields ?? []).some((f) => draft[f.key] !== String(instance.config[f.key] ?? ""));
  const save = () =>
    updateConfig(instance.id, {
      ...instance.config,
      ...Object.fromEntries((def.fields ?? []).map((f) => [f.key, f.number ? Number(draft[f.key]) || undefined : draft[f.key].trim()])),
    });
  if (!def.fields?.length) return null;
  return (
    <form
      className="svc-fields"
      onSubmit={(e) => {
        e.preventDefault();
        save();
      }}
    >
      {def.fields.map((f) => (
        <label key={f.key}>
          <span>{f.label}</span>
          <input
            value={draft[f.key]}
            placeholder={f.placeholder}
            inputMode={f.number ? "decimal" : undefined}
            spellCheck={false}
            onChange={(e) => setDraft({ ...draft, [f.key]: e.target.value })}
          />
        </label>
      ))}
      {changed && <button className="primary">Save</button>}
    </form>
  );
}

function money(v: number, cur?: string | null) {
  try {
    return new Intl.NumberFormat([], { style: "currency", currency: cur || "USD", maximumFractionDigits: Math.abs(v) >= 1000 ? 0 : 2 }).format(v);
  } catch {
    return v.toFixed(2);
  }
}

function ServicePanel({ instance, def }: WidgetProps & { def: ServiceDef }) {
  const { result, refresh, busy } = useReading(instance.id, def, instance.config);
  const [found, setFound] = useState<boolean | null>(null);
  useEffect(() => {
    if (def.source && !def.keyHint) invoke<boolean>("online_local_found", { provider: def.provider }).then(setFound);
  }, [def]);
  const r = result?.reading;
  const series = r?.series ?? [];
  const fmt =
    r?.currency
      ? (v: number) => money(v, r.currency)
      : def.provider === "currency" || def.provider === "stock" || def.provider === "crypto"
        ? (v: number) => (v >= 100 ? v.toFixed(2) : v.toPrecision(4))
        : (v: number) => (Number.isInteger(v) ? v.toLocaleString() : v.toFixed(1));

  return (
    <div className="panel w-ai-panel w-svc-panel">
      <div className="svc-head">
        <ServiceIcon service={def.icon} label={def.short} size={28} />
        <div>
          <h3>{def.name}</h3>
          {r && <span className="fine">Updated {time(r.fetched_at)}</span>}
        </div>
      </div>

      <Fields key={JSON.stringify(instance.config)} instance={instance} def={def} />

      {def.keyHint ? (
        <KeyForm key={instance.id} id={instance.id} svc={def} onSaved={() => refresh(true)} />
      ) : (
        def.source && (
          <p className="fine source">
            {def.source}
            {found === false && " Nothing was found on this PC yet."}
          </p>
        )
      )}

      {result?.error && (
        <p className="error">
          {result.error}
          {result.stale && r && ` Showing the last reading, from ${time(r.fetched_at)}.`}
        </p>
      )}

      {r && r.provider === def.provider && (
        <div className={`reading${result?.stale ? " stale" : ""}`}>
          {r.meters.length === 0 && def.view !== "calendar" && (
            <div className="headline">
              <b>{r.headline}</b>
              <span>{r.caption}</span>
            </div>
          )}
          {r.meters.map((m) => (
            <div key={m.label} className="meter-row">
              <div className="meter-label">
                <span>{m.label}</span>
                <small>{until(m.resets_at)}</small>
                <b>{Math.round(m.percent)}%</b>
              </div>
              <MeterBar m={m} />
            </div>
          ))}
          {def.view === "grid" ? (
            <ContributionGrid values={series} labels={r.series_labels ?? []} weeks={26} />
          ) : (
            series.length > 1 && <SeriesChart zero={def.view !== "change" && def.provider !== "currency"} values={series} labels={r.series_labels?.length === series.length ? r.series_labels : undefined} format={fmt} />
          )}
          {r.lines.map(([label, value], i) => (
            <div key={`${label}${i}`} className="line">
              <span>{label}</span>
              <b>{value}</b>
            </div>
          ))}
          {!!r.items?.length && (
            <ul className="svc-items">
              {r.items.map((it, i) => (
                <li key={i}>
                  <button disabled={!it.url} onClick={() => it.url && openLink(it.url)}>
                    <span className="t">
                      <b>{it.title}</b>
                      {it.detail && <small>{it.detail}</small>}
                    </span>
                    <span className={`v${isDown(it.value) ? " neg" : ""}`}>{def.view === "calendar" ? fmtWhen(it.value) : it.value}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      <div className="actions">
        <span className="fine">{def.every && def.every >= 600_000 ? `Checks every ${def.every / 60_000} minutes` : "Checks every minute"}</span>
        <button className="refresh" disabled={busy} onClick={() => refresh(true)}>
          {busy ? "Checking…" : "Refresh"}
        </button>
      </div>
    </div>
  );
}

export function serviceWidget(def: ServiceDef) {
  return {
    Tile: (p: WidgetProps) => <ServiceTile {...p} def={def} />,
    Panel: (p: WidgetProps) => <ServicePanel {...p} def={def} />,
  };
}

// ---- service definitions ---------------------------------------------------------------

const rev = (provider: string, short: string, icon: string, keyHint: string, extra: Partial<ServiceDef> = {}): ServiceDef => ({
  provider,
  short,
  name: `${short} revenue`,
  icon,
  view: "value",
  keyHint,
  every: 10 * 60_000,
  demo: {
    headline: "$4,280",
    caption: `${short} net revenue, last 30 days`,
    value: 4280,
    currency: "USD",
    series: [120, 90, 160, 140, 210, 180, 260, 230, 190, 280, 320, 300, 350, 410],
  },
  ...extra,
});

export const DEFS: Record<string, ServiceDef> = {
  stripe: rev("stripe", "Stripe", "stripe", "A restricted key with read access to Balance transactions (starts rk_live_), from the Stripe Dashboard → Developers → API keys."),
  polar: rev("polar", "Polar", "polar", "An organization access token with metrics:read, from Polar → Settings → Developers."),
  dodo: rev("dodo", "Dodo", "dodo", "An API key from Dodo Payments → Developer → API keys."),
  shopify: rev("shopify", "Shopify", "shopify", "An Admin API access token from a custom app with the read_orders scope (Shopify admin → Settings → Apps → Develop apps).", {
    fields: [{ key: "shop", label: "Store address", placeholder: "yourstore.myshopify.com" }],
  }),
  lemonsqueezy: rev("lemonsqueezy", "Lemon Squeezy", "lemonsqueezy", "An API key from Lemon Squeezy → Settings → API."),
  paddle: rev("paddle", "Paddle", "paddle", "A Paddle Billing API key that can read transactions (Paddle → Developer tools → Authentication)."),
  gumroad: rev("gumroad", "Gumroad", "gumroad", "An access token from Gumroad → Settings → Advanced → Applications → Generate access token."),

  gemini: {
    provider: "gemini",
    short: "Gemini",
    name: "Gemini CLI quota",
    icon: "gemini",
    view: "meters",
    source: "Read with Gemini CLI's own Google sign-in on this PC, from the quota Gemini CLI itself checks. The sign-in is only read, never stored or shown. If it has expired, run gemini once.",
    every: 5 * 60_000,
    demo: { headline: "12%", caption: "Gemini CLI, Pro used", meters: [{ label: "Pro", percent: 12, resets_at: null }, { label: "Flash", percent: 4, resets_at: null }] },
  },
  glm: {
    provider: "glm",
    short: "GLM",
    name: "GLM Coding Plan",
    icon: "glm",
    view: "meters",
    keyHint: "Your Z.ai (or Zhipu BigModel) Coding Plan API key.",
    every: 2 * 60_000,
    demo: { headline: "27%", caption: "GLM Coding Plan, session", meters: [{ label: "Session", percent: 27, resets_at: null }, { label: "Tools, monthly", percent: 8, resets_at: null }] },
  },
  opencode: {
    provider: "opencode",
    short: "OpenCode",
    name: "OpenCode session",
    icon: "opencode",
    view: "value",
    source: "Read from the history OpenCode keeps on this PC. Nothing is sent anywhere.",
    demo: { headline: "184K", caption: "OpenCode tokens, latest session" },
  },
  opencode_week: {
    provider: "opencode_week",
    short: "OpenCode",
    name: "OpenCode usage",
    icon: "opencode",
    view: "chart",
    source: "Read from the history OpenCode keeps on this PC. Nothing is sent anywhere.",
    demo: { headline: "2.4M", caption: "OpenCode tokens, last 7 days", series: [180, 420, 300, 510, 260, 390, 340] },
  },

  stock: {
    provider: "stock",
    short: "Stock",
    name: "Stock",
    icon: "stock",
    view: "change",
    source: "Prices from Yahoo Finance's public charts, a few minutes delayed. Only the ticker is sent.",
    fields: [{ key: "symbol", label: "Ticker", placeholder: "AAPL" }],
    demo: { headline: "$231.40", caption: "AAPL +1.24%", series: [228, 229, 228.5, 230, 229.6, 230.8, 231.4] },
  },
  watchlist: {
    provider: "watchlist",
    short: "Watchlist",
    name: "Stock watchlist",
    icon: "stock",
    view: "list",
    source: "Prices from Yahoo Finance's public charts, a few minutes delayed. Only the tickers are sent.",
    fields: [{ key: "symbols", label: "Tickers, separated by commas", placeholder: "AAPL, MSFT, NVDA" }],
    demo: {
      headline: "$231.40",
      caption: "AAPL · MSFT · NVDA",
      items: [
        { title: "AAPL", detail: "+1.24%", value: "$231.40", url: null },
        { title: "MSFT", detail: "-0.41%", value: "$418.02", url: null },
        { title: "NVDA", detail: "+2.10%", value: "$142.77", url: null },
      ],
    },
  },
  crypto: {
    provider: "crypto",
    short: "Crypto",
    name: "Crypto price",
    icon: "binance",
    view: "change",
    source: "Prices from Binance's public market data. Only the coin is sent.",
    fields: [{ key: "symbol", label: "Coin", placeholder: "BTC" }],
    demo: { headline: "$97,120", caption: "BTCUSDT +2.31%", series: [94, 95, 94.6, 96, 95.5, 96.8, 97.1] },
  },
  currency: {
    provider: "currency",
    short: "Currency",
    name: "Currency",
    icon: "currency",
    view: "value",
    source: "Rates from the European Central Bank, via Frankfurter. Updated once a working day.",
    every: 60 * 60_000,
    fields: [
      { key: "from", label: "From", placeholder: "USD" },
      { key: "to", label: "To", placeholder: "EUR" },
      { key: "amount", label: "Amount", placeholder: "1", number: true },
    ],
    demo: { headline: "0.95 EUR", caption: "1 USD in EUR" },
  },

  plausible: {
    provider: "plausible",
    short: "Plausible",
    name: "Plausible visitors",
    icon: "plausible",
    view: "chart",
    keyHint: "A Stats API key from Plausible → Account settings → API keys.",
    fields: [
      { key: "site", label: "Site", placeholder: "example.com" },
      { key: "base", label: "Self-hosted address (optional)", placeholder: "https://plausible.io" },
    ],
    every: 10 * 60_000,
    demo: { headline: "3.2K", caption: "example.com, visitors this week", series: [380, 420, 510, 460, 390, 520, 480] },
  },
  dub: {
    provider: "dub",
    short: "Dub",
    name: "Dub link clicks",
    icon: "dub",
    view: "chart",
    keyHint: "An API key from Dub → Settings → API keys, allowed to read analytics.",
    every: 10 * 60_000,
    demo: { headline: "1.8K", caption: "Dub link clicks this week", series: [210, 260, 230, 300, 280, 250, 270] },
  },

  resend: {
    provider: "resend",
    short: "Resend",
    name: "Resend emails",
    icon: "resend",
    view: "value",
    keyHint: "A Resend API key with full access (a sending-only key can't list emails).",
    every: 5 * 60_000,
    demo: { headline: "1.2K", caption: "Resend emails sent this week", lines: [["Delivered", "1,184 (98%)"]] },
  },
  linear: {
    provider: "linear",
    short: "Linear",
    name: "Linear issues",
    icon: "linear",
    view: "list",
    keyHint: "A personal API key from Linear → Settings → Security & access.",
    every: 2 * 60_000,
    demo: {
      headline: "7",
      caption: "Linear issues assigned to you",
      items: [{ title: "Fix sign-in redirect", detail: "ENG-142", value: "In Progress", url: null }],
    },
  },
  supabase: {
    provider: "supabase",
    short: "Supabase",
    name: "Supabase project",
    icon: "supabase",
    view: "chart",
    keyHint: "A personal access token from supabase.com/dashboard/account/tokens.",
    fields: [{ key: "project", label: "Project reference", placeholder: "abcdefghijklmnopqrst" }],
    every: 5 * 60_000,
    demo: { headline: "48K", caption: "My app: API requests, Healthy", series: [5, 7, 6, 8, 7, 9, 6] },
  },

  mail: {
    provider: "mail",
    short: "Mail",
    name: "Unread mail",
    icon: "gmail",
    view: "list",
    keyLabel: "app password",
    keyHint:
      "An app password, not your normal password. Gmail: myaccount.google.com/apppasswords (needs 2-Step Verification). iCloud: account.apple.com → App-Specific Passwords. Works with any mail that offers IMAP.",
    fields: [
      { key: "email", label: "Email address", placeholder: "you@gmail.com" },
      { key: "server", label: "IMAP server (optional)", placeholder: "imap.gmail.com" },
    ],
    every: 2 * 60_000,
    demo: { headline: "3", caption: "Unread mail", items: [{ title: "Your order has shipped", detail: "Store", value: "", url: null }] },
  },
  github_activity: {
    provider: "github_activity",
    short: "GitHub",
    name: "GitHub activity",
    icon: "github",
    view: "grid",
    keyHint: "A GitHub personal access token (fine-grained with no extra permissions is enough).",
    every: 15 * 60_000,
    demo: {
      headline: "12d",
      caption: "Your GitHub streak",
      series: Array.from({ length: 182 }, (_, i) => ((i * 7) % 11 > 4 ? (i * 3) % 9 : 0)),
      series_labels: Array.from({ length: 182 }, (_, i) => new Date(Date.now() - (181 - i) * 864e5).toISOString().slice(0, 10)),
    },
  },
  github_notifications: {
    provider: "github_notifications",
    short: "GitHub",
    name: "GitHub notifications",
    icon: "github",
    view: "list",
    keyHint: "A classic GitHub token with the notifications scope (fine-grained tokens can't read notifications).",
    every: 2 * 60_000,
    demo: { headline: "5", caption: "Unread GitHub notifications", items: [{ title: "Review requested", detail: "you/app", value: "review requested", url: null }] },
  },
  github_overview: {
    provider: "github_overview",
    short: "GitHub",
    name: "GitHub overview",
    icon: "github",
    view: "value",
    keyHint: "A classic GitHub token with the repo and notifications scopes.",
    every: 10 * 60_000,
    demo: { headline: "★ 1.2K", caption: "You on GitHub", lines: [["Notifications", "5"], ["Open pull requests", "3"]] },
  },
  leetcode: {
    provider: "leetcode",
    short: "LeetCode",
    name: "LeetCode stats",
    icon: "leetcode",
    view: "value",
    source: "Public profile data from LeetCode. Only the username is sent.",
    fields: [{ key: "user", label: "LeetCode username", placeholder: "username" }],
    every: 30 * 60_000,
    demo: { headline: "412", caption: "Problems solved", lines: [["Easy", "180"], ["Medium", "190"], ["Hard", "42"]] },
  },

  x: {
    provider: "x",
    short: "X",
    name: "X followers",
    icon: "x",
    view: "value",
    keyHint: "An X API bearer token. User lookups need an X developer plan that includes them.",
    fields: [{ key: "user", label: "Username", placeholder: "@you" }],
    every: 30 * 60_000,
    demo: { headline: "8.4K", caption: "@you followers on X" },
  },
  youtube: {
    provider: "youtube",
    short: "YouTube",
    name: "YouTube subscribers",
    icon: "youtube",
    view: "value",
    keyHint: "A YouTube Data API v3 key from Google Cloud Console → APIs & Services → Credentials.",
    fields: [{ key: "channel", label: "Channel handle", placeholder: "@yourchannel" }],
    every: 30 * 60_000,
    demo: { headline: "12.5K", caption: "Your channel subscribers" },
  },
  instagram: {
    provider: "instagram",
    short: "Instagram",
    name: "Instagram followers",
    icon: "instagram",
    view: "value",
    keyHint: "An Instagram API access token for a professional account (Instagram API with Instagram Login, from Meta for Developers).",
    every: 30 * 60_000,
    demo: { headline: "3.1K", caption: "@you followers on Instagram" },
  },

  calendar: {
    provider: "calendar",
    short: "Calendar",
    name: "Calendar",
    icon: "calendar",
    view: "calendar",
    keyLabel: "calendar address",
    keyHint:
      "Your calendar's private address in iCal format. Google Calendar: Settings → your calendar → Secret address in iCal format. Outlook: Settings → Shared calendars → Publish a calendar → ICS. iCloud: share the calendar as Public. Times in another time zone are shown as written.",
    every: 10 * 60_000,
    demo: {
      headline: new Date(Date.now() + 2 * 3600e3).toISOString().slice(0, 13) + ":00",
      caption: "Design review",
      items: [{ title: "Design review", detail: "Room 2", value: "", url: null }],
    },
  },
};

// Chart versions of the revenue widgets.
for (const p of ["stripe", "polar", "dodo", "shopify", "lemonsqueezy", "paddle", "gumroad"]) {
  DEFS[`${p}_chart`] = { ...DEFS[p], view: "chart", name: `${DEFS[p].short} revenue chart` };
}

// ---- Total revenue ----------------------------------------------------------------------

const REVENUE_KINDS = Object.keys(DEFS).filter((k) => DEFS[k].name.includes("revenue"));
type TotalConfig = { goal?: number };

function useTotals(preview?: boolean) {
  const settings = useSettings();
  const [rows, setRows] = useState<{ kind: string; value: number; currency: string; stale: boolean }[]>([]);
  const ids = useMemo(
    () => (settings?.widgets ?? []).filter((w) => REVENUE_KINDS.includes(w.kind)).map((w) => [w.id, w.kind] as const),
    [settings],
  );
  useEffect(() => {
    if (preview) return;
    const load = () =>
      Promise.all(ids.map(([id, kind]) => invoke<Result | null>("widget_data_get", { id }).then((d) => ({ d, kind })))).then((all) =>
        setRows(
          all
            .filter((x) => x.d?.reading?.value != null)
            .map((x) => ({ kind: x.kind, value: x.d!.reading!.value!, currency: x.d!.reading!.currency ?? "USD", stale: !!x.d!.stale })),
        ),
      );
    load();
    const t = setInterval(load, 60_000);
    return () => clearInterval(t);
  }, [ids, preview]);
  if (preview) return { rows: [{ kind: "stripe", value: 3120, currency: "USD", stale: false }, { kind: "polar", value: 1160, currency: "USD", stale: false }], ids };
  return { rows, ids };
}

function total(rows: { value: number; currency: string; kind: string }[]) {
  const by: Record<string, number> = {};
  for (const r of rows) by[r.currency] = (by[r.currency] ?? 0) + r.value;
  const main = Object.entries(by).sort((a, b) => b[1] - a[1])[0];
  return { cur: main?.[0] ?? "USD", sum: main?.[1] ?? 0, other: Object.keys(by).length > 1 };
}

export function TotalRevenueTile({ instance, preview }: WidgetProps<TotalConfig>) {
  const { rows, ids } = useTotals(preview);
  const t = total(rows);
  const goal = instance.config.goal ?? (preview ? 5000 : undefined);
  if (!preview && !ids.length) return <div className="w-svc setup-only">Add a store</div>;
  return (
    <div className={`w-svc ${instance.size} v-total`} title={`Revenue, last 30 days, from ${rows.length} store${rows.length === 1 ? "" : "s"}`}>
      <span className="glyph">{glyph.currency}</span>
      <span className="value">
        <b>{money(t.sum, t.cur)}</b>
        {instance.size !== "s" && goal ? <small>{Math.round((t.sum / goal) * 100)}% of goal</small> : instance.size === "l" && <small>30 days</small>}
      </span>
    </div>
  );
}

export function TotalRevenuePanel({ instance }: WidgetProps<TotalConfig>) {
  const { rows, ids } = useTotals();
  const t = total(rows);
  const [goal, setGoal] = useState(String(instance.config.goal ?? ""));
  const day = new Date().getDate();
  const daysInMonth = new Date(new Date().getFullYear(), new Date().getMonth() + 1, 0).getDate();
  const g = instance.config.goal;
  return (
    <div className="panel w-ai-panel w-svc-panel">
      <h3>Total revenue</h3>
      {!ids.length && <p className="fine">Add one or more revenue widgets (Stripe, Polar, Shopify…) with their keys; this adds them up.</p>}
      <div className="reading">
        <div className="headline">
          <b>{money(t.sum, t.cur)}</b>
          <span>Last 30 days, all stores{t.other ? ` (${t.cur} only)` : ""}</span>
        </div>
        {g ? (
          <div className="meter-row">
            <div className="meter-label">
              <span>Monthly goal {money(g, t.cur)}</span>
              <small>pace {Math.round(((t.sum / 30) * daysInMonth / g) * 100)}% · day {day}</small>
              <b>{Math.round((t.sum / g) * 100)}%</b>
            </div>
            <span className="meter ok">
              <i style={{ width: `${Math.min(100, (t.sum / g) * 100)}%` }} />
            </span>
          </div>
        ) : null}
        {rows.map((r, i) => (
          <div key={i} className="line">
            <span>
              {DEFS[r.kind]?.short ?? r.kind}
              {r.stale ? " (last reading)" : ""}
            </span>
            <b>{money(r.value, r.currency)}</b>
          </div>
        ))}
      </div>
      <form
        className="svc-fields"
        onSubmit={(e) => {
          e.preventDefault();
          updateConfig(instance.id, { ...instance.config, goal: Number(goal) || undefined });
        }}
      >
        <label>
          <span>Monthly goal</span>
          <input value={goal} inputMode="decimal" placeholder="5000" onChange={(e) => setGoal(e.target.value)} />
        </label>
        {goal !== String(instance.config.goal ?? "") && <button className="primary">Save</button>}
      </form>
      <p className="fine">Adds up the revenue widgets on your dock. Amounts in other currencies aren't converted.</p>
    </div>
  );
}

// ---- AI limits: two plans in one tile ---------------------------------------------------------

const PLANS: Record<string, { short: string; icon: string }> = {
  claude_code: { short: "Claude", icon: "claude" },
  codex: { short: "ChatGPT", icon: "openai" },
  gemini: { short: "Gemini", icon: "gemini" },
};
type LimitsConfig = { a?: string; b?: string };

function usePlans(id: string, cfg: LimitsConfig, preview?: boolean) {
  const a = cfg.a ?? "claude_code";
  const b = cfg.b ?? "codex";
  const [res, setRes] = useState<Record<string, Result>>({});
  const load = useCallback(
    (manual = false) => {
      if (preview) return;
      for (const [slot, p] of [
        ["a", a],
        ["b", b],
      ] as const) {
        invoke<Result>("ai_usage_fetch", { id: `${id}${slot}`, provider: p, manual }).then((r) => setRes((m) => ({ ...m, [slot]: r })));
      }
    },
    [id, a, b, preview],
  );
  useEffect(() => {
    load();
    const t = setInterval(() => load(), 5 * 60_000);
    return () => clearInterval(t);
  }, [load]);
  if (preview) {
    const mk = (p: number) => ({ reading: { meters: [{ label: "Session", percent: p, resets_at: null }] } as Reading, error: null, stale: false });
    return { a, b, res: { a: mk(34), b: mk(18) } as Record<string, Result>, load };
  }
  return { a, b, res, load };
}

const first = (r?: Result) => r?.reading?.meters.find((m) => m.label === "Session") ?? r?.reading?.meters[0];

export function AiLimitsTile({ instance, preview }: WidgetProps<LimitsConfig>) {
  const { a, b, res } = usePlans(instance.id, instance.config, preview);
  return (
    <div className={`w-ai multi ${instance.size}`}>
      <span className="mrows">
        {(
          [
            ["a", a],
            ["b", b],
          ] as const
        ).map(([slot, p]) => {
          const m = first(res[slot]);
          return (
            <span key={slot} className="mrow" title={res[slot]?.error ?? ""}>
              <ServiceIcon service={PLANS[p]?.icon ?? p} label={PLANS[p]?.short ?? p} size={13} />
              {instance.size !== "s" && <MeterBar m={m ?? { label: "", percent: 0, resets_at: null }} thin />}
              <b>{m ? `${Math.round(m.percent)}%` : "–"}</b>
            </span>
          );
        })}
      </span>
    </div>
  );
}

export function AiLimitsPanel({ instance }: WidgetProps<LimitsConfig>) {
  const { a, b, res, load } = usePlans(instance.id, instance.config);
  const pick = (slot: "a" | "b", value: string) => updateConfig(instance.id, { ...instance.config, [slot]: value });
  return (
    <div className="panel w-ai-panel">
      <h3>AI limits</h3>
      {(
        [
          ["a", a],
          ["b", b],
        ] as const
      ).map(([slot, p]) => (
        <div key={slot} className="plan-block">
          <select value={p} onChange={(e) => pick(slot, e.target.value)}>
            {Object.entries(PLANS).map(([id, x]) => (
              <option key={id} value={id}>
                {x.short}
              </option>
            ))}
          </select>
          {res[slot]?.error && <p className="error">{res[slot].error}</p>}
          {res[slot]?.reading?.meters.map((m) => (
            <div key={m.label} className="meter-row">
              <div className="meter-label">
                <span>{m.label}</span>
                <small>{until(m.resets_at)}</small>
                <b>{Math.round(m.percent)}%</b>
              </div>
              <MeterBar m={m} />
            </div>
          ))}
        </div>
      ))}
      <div className="actions">
        <span className="fine">Uses each app's own sign-in on this PC</span>
        <button className="refresh" onClick={() => load(true)}>
          Refresh
        </button>
      </div>
    </div>
  );
}
