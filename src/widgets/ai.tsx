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

const PROVIDERS: { id: Provider; short: string; name: string; keyHint?: string; every: number }[] = [
  {
    id: "anthropic",
    short: "Claude API",
    name: "Claude API spend",
    keyHint: "An Admin API key (starts sk-ant-admin), from the Claude Console under Settings → Admin keys. Data arrives 4–24 hours late.",
    every: 30 * 60_000,
  },
  {
    id: "openai",
    short: "OpenAI",
    name: "OpenAI API spend",
    keyHint: "An OpenAI admin key, from the OpenAI platform's organization settings.",
    every: 30 * 60_000,
  },
  { id: "deepseek", short: "DeepSeek", name: "DeepSeek balance", keyHint: "Your DeepSeek API key.", every: 15 * 60_000 },
  { id: "claude_code", short: "Claude plan", name: "Claude plan limits (experimental)", every: 5 * 60_000 },
];
const providerOf = (cfg: AiConfig) => PROVIDERS.find((p) => p.id === (cfg.provider ?? "anthropic"))!;

const DEMO: UsageResult = {
  reading: {
    provider: "claude_code",
    headline: "42%",
    caption: "Claude plan, 5-hour window",
    lines: [],
    meters: [{ label: "5-hour window", percent: 42, resets_at: null }],
    data_as_of: null,
    fetched_at: Date.now(),
  },
  error: null,
  stale: false,
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

/** Shared between the tile and the panel: the last result, refreshed on a timer by the tile. */
function useUsage(id: string, provider: Provider, preview?: boolean) {
  const [result, save] = useWidgetData<UsageResult | null>(id, preview ? DEMO : null, preview);
  const [busy, setBusy] = useState(false);
  const refresh = useCallback(async () => {
    if (preview) return;
    setBusy(true);
    try {
      save(await invoke<UsageResult>("ai_usage_fetch", { id, provider }));
    } finally {
      setBusy(false);
    }
  }, [id, provider, preview, save]);
  return { result, refresh, busy };
}

export function AiTile({ instance, preview }: WidgetProps<AiConfig>) {
  const p = providerOf(instance.config);
  const { result, refresh } = useUsage(instance.id, p.id, preview);

  // The dock's tile owns the timer, so each provider is asked at most once per interval.
  useEffect(() => {
    if (preview) return;
    refresh();
    const t = setInterval(refresh, p.every);
    return () => clearInterval(t);
  }, [p.id, preview]);

  const r = result?.reading;
  const stale = !!result?.stale;
  const failedNoData = result && !r;
  const title = r
    ? `${r.caption}: ${r.headline}${stale ? `\nLast reading at ${time(r.fetched_at)} — ${result?.error}` : ""}`
    : result?.error ?? p.name;
  // Plan limits: one row per window (Session, Weekly…) so both are visible at once.
  const meters = r?.meters ?? [];
  if (meters.length > 1 && instance.size !== "s") {
    return (
      <div className={`w-ai multi ${instance.size}${stale ? " stale" : ""}`} title={title}>
        {meters.slice(0, 2).map((m) => (
          <span key={m.label} className="mrow">
            <small>{m.label}</small>
            <MeterBar m={m} thin />
            <b>{Math.round(m.percent)}%</b>
          </span>
        ))}
        {stale && <span className="glyph warn">{""}</span>}
      </div>
    );
  }
  const meter = meters[0];

  return (
    <div className={`w-ai ${instance.size}${stale ? " stale" : ""}`} title={title}>
      {instance.size !== "s" && <span className="who">{p.short}</span>}
      {failedNoData ? (
        <span className="setup">{result?.error === "Add a key to start." ? "Add key" : "Can't read"}</span>
      ) : (
        <span className="value">
          <b>{r?.headline ?? "…"}</b>
          {stale && <span className="glyph warn">{""}</span>}
        </span>
      )}
      {meter && <MeterBar m={meter} thin />}
    </div>
  );
}

function KeyForm({ id, provider, onSaved }: { id: string; provider: (typeof PROVIDERS)[number]; onSaved: () => void }) {
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
      <p className="fine">{provider.keyHint} It's encrypted with your Windows account and only sent to that provider.</p>
      {error && <p className="error">{error}</p>}
    </form>
  );
}

export function AiPanel({ instance }: WidgetProps<AiConfig>) {
  const p = providerOf(instance.config);
  const { result, refresh, busy } = useUsage(instance.id, p.id);
  const [found, setFound] = useState<boolean | null>(null);
  useEffect(() => {
    if (p.id === "claude_code") invoke<boolean>("ai_claude_code_found").then(setFound);
  }, [p.id]);
  const r = result?.reading;

  return (
    <div className="panel w-ai-panel">
      <h3>AI usage</h3>
      <select
        value={p.id}
        onChange={(e) => {
          updateConfig(instance.id, { ...instance.config, provider: e.target.value });
        }}
      >
        {PROVIDERS.map((x) => (
          <option key={x.id} value={x.id}>
            {x.name}
          </option>
        ))}
      </select>

      {p.id === "claude_code" ? (
        <div className="experimental">
          <b>Experimental.</b> Reads Claude Code's own sign-in on this PC and asks an internal Anthropic endpoint for your plan
          limits. It can change or stop working without notice. The sign-in is only read, never stored or shown.
          <div className="found">
            {found === null ? "" : found ? "Claude Code sign-in found." : "No Claude Code sign-in found on this PC."}
          </div>
        </div>
      ) : (
        <KeyForm key={`${instance.id}-${p.id}`} id={instance.id} provider={p} onSaved={refresh} />
      )}

      {result?.error && (
        <p className="error">
          {result.error}
          {result.stale && r && ` Showing the last reading, from ${time(r.fetched_at)}.`}
        </p>
      )}

      {r && r.provider === p.id && (
        <div className={`reading${result?.stale ? " stale" : ""}`}>
          <div className="headline">
            <b>{r.headline}</b>
            <span>{r.caption}</span>
          </div>
          {r.meters.map((m) => (
            <div key={m.label} className="meter-row">
              <div className="meter-label">
                <span>{m.label}</span>
                <b>{Math.round(m.percent)}%</b>
              </div>
              <MeterBar m={m} />
              <small>{until(m.resets_at)}</small>
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
          {r ? `Updated ${time(r.fetched_at)}` : ""}
          {r?.data_as_of ? ` · data up to ${new Date(r.data_as_of).toLocaleString([], { dateStyle: "short", timeStyle: "short" })}` : ""}
        </span>
        <button className="link" disabled={busy} onClick={refresh}>
          {busy ? "Checking…" : "Refresh"}
        </button>
      </div>
    </div>
  );
}
