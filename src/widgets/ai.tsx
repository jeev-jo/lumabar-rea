import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { updateConfig, useWidgetData, type WidgetProps } from "./shared";

export type Provider = "anthropic" | "openai" | "deepseek" | "claude_code";
export type AiConfig = { provider?: Provider };

type Meter = { label: string; percent: number; resets_at: string | null };
type Reading = {
  provider: Provider;
  headline: string;
  caption: string;
  lines: [string, string][];
  meters: Meter[];
  data_as_of: string | null;
  fetched_at: number;
};
type UsageResult = { reading: Reading | null; error: string | null; stale: boolean };

type Service = {
  id: Provider;
  /** Short name on the tile. */
  short: string;
  /** Full name in the details view. */
  name: string;
  /** Which site icon to show (see `service_icon` in Rust). */
  icon: string;
  keyHint?: string;
  source?: string;
};

export const SERVICES: Record<Provider, Service> = {
  claude_code: {
    id: "claude_code",
    short: "Claude",
    name: "Claude plan limits",
    icon: "claude",
    source: "Read from your Claude Code sign-in on this PC. The sign-in is only read, never stored or shown.",
  },
  anthropic: {
    id: "anthropic",
    short: "Claude API",
    name: "Claude API spend",
    icon: "claude",
    keyHint: "An Admin API key (starts sk-ant-admin), from the Claude Console under Settings → Admin keys. Data arrives 4–24 hours late.",
  },
  openai: {
    id: "openai",
    short: "OpenAI",
    name: "OpenAI API spend",
    icon: "openai",
    keyHint: "An OpenAI admin key, from the OpenAI platform's organization settings.",
  },
  deepseek: { id: "deepseek", short: "DeepSeek", name: "DeepSeek balance", icon: "deepseek", keyHint: "Your DeepSeek API key." },
};

/** Every service refreshes once a minute; the dock's tile owns the timer. */
const EVERY = 60_000;

const demo = (provider: Provider, headline: string, caption: string, meters: Meter[] = []): UsageResult => ({
  reading: { provider, headline, caption, lines: [], meters, data_as_of: null, fetched_at: Date.now() },
  error: null,
  stale: false,
});

const DEMO: Record<Provider, UsageResult> = {
  claude_code: demo("claude_code", "34%", "Claude plan, session", [
    { label: "Session", percent: 34, resets_at: null },
    { label: "Weekly", percent: 21, resets_at: null },
  ]),
  anthropic: demo("anthropic", "$12.40", "Claude API spend this month"),
  openai: demo("openai", "$8.15", "OpenAI API spend this month"),
  deepseek: demo("deepseek", "$9.80", "DeepSeek balance"),
};

const time = (ms: number | string) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

function until(iso: string | null) {
  if (!iso) return "";
  const mins = Math.round((new Date(iso).getTime() - Date.now()) / 60_000);
  if (Number.isNaN(mins)) return "";
  if (mins <= 0) return "resets now";
  const d = Math.floor(mins / 1440);
  const h = Math.floor((mins % 1440) / 60);
  const m = mins % 60;
  return `resets in ${d ? `${d}d ` : ""}${h ? `${h}h ` : ""}${!d ? `${m}m` : ""}`.trim();
}

const level = (p: number) => (p >= 90 ? "critical" : p >= 70 ? "warning" : "ok");

function MeterBar({ m, thin }: { m: Meter; thin?: boolean }) {
  return (
    <span className={`meter ${level(m.percent)}${thin ? " thin" : ""}`}>
      <i style={{ width: `${Math.max(2, m.percent)}%` }} />
    </span>
  );
}

// ---- service icons: fetched once by Rust from each service's own site ----

const iconCache = new Map<string, Promise<string | null>>();

export function ServiceIcon({ service, label, size = 20 }: { service: string; label: string; size?: number }) {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    let p = iconCache.get(service);
    if (!p) {
      p = invoke<string | null>("service_icon", { service }).catch(() => null);
      iconCache.set(service, p);
    }
    p.then((s) => live && setSrc(s));
    return () => {
      live = false;
    };
  }, [service]);
  return src ? (
    <img className="svc-icon" src={src} alt="" width={size} height={size} draggable={false} />
  ) : (
    <span className="svc-icon letter" style={{ width: size, height: size }}>
      {label.slice(0, 1)}
    </span>
  );
}

/** The last result, shared by the tile and the details view; the tile refreshes it. */
function useUsage(id: string, provider: Provider, preview?: boolean) {
  const [result, save] = useWidgetData<UsageResult | null>(id, preview ? DEMO[provider] : null, preview);
  const [busy, setBusy] = useState(false);
  const refresh = useCallback(async (manual = false) => {
    if (preview) return;
    setBusy(true);
    try {
      save(await invoke<UsageResult>("ai_usage_fetch", { id, provider, manual }));
    } finally {
      setBusy(false);
    }
  }, [id, provider, preview, save]);
  return { result, refresh, busy };
}

function Tile({ instance, preview, provider }: WidgetProps<AiConfig> & { provider: Provider }) {
  const svc = SERVICES[provider];
  const { result, refresh } = useUsage(instance.id, provider, preview);

  useEffect(() => {
    if (preview) return;
    refresh();
    const t = setInterval(refresh, EVERY);
    return () => clearInterval(t);
  }, [provider, preview]);

  const r = result?.reading;
  const stale = !!result?.stale;
  const title = r
    ? `${r.caption}: ${r.headline}${stale ? `\nLast reading at ${time(r.fetched_at)} — ${result?.error}` : ""}`
    : result?.error ?? svc.name;
  const meters = r?.meters ?? [];
  const icon = <ServiceIcon service={svc.icon} label={svc.short} size={instance.size === "s" ? 18 : 22} />;

  // Plan limits: only Session and Weekly in the dock; extra usage and the
  // per-model limits are in the details.
  const main = meters.filter((m) => m.label === "Session" || m.label === "Weekly");
  const shown = main.length > 1 ? main : meters.slice(0, 2);
  if (shown.length > 1 && instance.size !== "s") {
    return (
      <div className={`w-ai multi ${instance.size}${stale ? " stale" : ""}`} title={title}>
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

  return (
    <div className={`w-ai ${instance.size}${stale ? " stale" : ""}`} title={title}>
      {icon}
      {result && !r ? (
        <span className="setup">{result.error === "Add a key to start." ? "Add key" : "Can't read"}</span>
      ) : (
        <span className="value">
          <b>{r?.headline ?? "…"}</b>
          {instance.size === "l" && <small>{svc.short}</small>}
        </span>
      )}
    </div>
  );
}

function KeyForm({ id, svc, onSaved }: { id: string; svc: Service; onSaved: () => void }) {
  const [has, setHas] = useState<boolean | null>(null);
  const [key, setKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    invoke<boolean>("ai_key_status", { id }).then(setHas);
  }, [id]);
  if (has === null) return null;
  if (has) {
    return (
      <div className="key-row">
        <span>
          <span className="glyph ok">{""}</span> Key saved, encrypted on this PC
        </span>
        <button className="link" onClick={() => invoke("ai_clear_key", { id }).then(() => setHas(false))}>
          Remove key
        </button>
      </div>
    );
  }
  return (
    <form
      className="key-form"
      onSubmit={(e) => {
        e.preventDefault();
        invoke("ai_set_key", { id, key })
          .then(() => {
            setKey("");
            setError(null);
            setHas(true);
            onSaved();
          })
          .catch((err) => setError(String(err)));
      }}
    >
      <input type="password" autoComplete="off" spellCheck={false} placeholder="Paste key" value={key} onChange={(e) => setKey(e.target.value)} />
      <button className="primary" disabled={!key.trim()}>
        Save
      </button>
      <p className="fine">{svc.keyHint} It's encrypted with your Windows account and only sent to that service.</p>
      {error && <p className="error">{error}</p>}
    </form>
  );
}

function Panel({ instance, provider, chooser }: WidgetProps<AiConfig> & { provider: Provider; chooser?: boolean }) {
  const svc = SERVICES[provider];
  const { result, refresh, busy } = useUsage(instance.id, provider);
  const [found, setFound] = useState<boolean | null>(null);
  useEffect(() => {
    if (provider === "claude_code") invoke<boolean>("ai_claude_code_found").then(setFound);
  }, [provider]);
  const r = result?.reading;

  return (
    <div className="panel w-ai-panel">
      <div className="svc-head">
        <ServiceIcon service={svc.icon} label={svc.short} size={28} />
        <div>
          <h3>{svc.name}</h3>
          {r && (
            <span className="fine">
              Updated {time(r.fetched_at)} · checks every {provider === "claude_code" ? "5 minutes" : "minute"}
            </span>
          )}
        </div>
      </div>

      {chooser && (
        <select value={provider} onChange={(e) => updateConfig(instance.id, { ...instance.config, provider: e.target.value })}>
          {Object.values(SERVICES).map((x) => (
            <option key={x.id} value={x.id}>
              {x.name}
            </option>
          ))}
        </select>
      )}

      {provider === "claude_code" ? (
        <p className="fine source">
          {svc.source}
          {found === false && " No Claude Code sign-in was found — sign in to Claude Code first."}
        </p>
      ) : (
        <KeyForm key={`${instance.id}-${provider}`} id={instance.id} svc={svc} onSaved={() => refresh(true)} />
      )}

      {result?.error && (
        <p className="error">
          {result.error}
          {result.stale && r && ` Showing the last reading, from ${time(r.fetched_at)}.`}
        </p>
      )}

      {r && r.provider === provider && (
        <div className={`reading${result?.stale ? " stale" : ""}`}>
          {r.meters.length === 0 && (
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
          {r.lines.map(([label, value], i) => (
            <div key={`${label}${i}`} className="line">
              <span>{label}</span>
              <b>{value}</b>
            </div>
          ))}
        </div>
      )}

      <div className="actions">
        <span className="fine">
          {r?.data_as_of ? `Data up to ${new Date(r.data_as_of).toLocaleString([], { dateStyle: "short", timeStyle: "short" })}` : ""}
        </span>
        <button className="refresh" disabled={busy} onClick={() => refresh(true)}>
          {busy ? "Checking…" : "Refresh"}
        </button>
      </div>
    </div>
  );
}

/** One widget kind per service: the tile and details view with that service fixed. */
export function aiWidget(provider: Provider) {
  return {
    Tile: (p: WidgetProps<AiConfig>) => <Tile {...p} provider={provider} />,
    Panel: (p: WidgetProps<AiConfig>) => <Panel {...p} provider={provider} />,
  };
}

/** The original "AI usage" widget, where the service is chosen in its settings. Kept so existing ones still work. */
export const AiTile = (p: WidgetProps<AiConfig>) => <Tile {...p} provider={p.instance.config.provider ?? "anthropic"} />;
export const AiPanel = (p: WidgetProps<AiConfig>) => <Panel {...p} provider={p.instance.config.provider ?? "anthropic"} chooser />;
