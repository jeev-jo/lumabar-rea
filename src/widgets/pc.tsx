/**
 * More PC widgets: CPU, memory and network on their own, four battery looks,
 * top CPU and top memory apps, day/month/year progress, keyboard language,
 * temperature, brightness, notifications, the on-screen keyboard, three
 * compact now-playing tiles, and apps & files (apps, recent files, search,
 * commands, screenshots, wallpaper gallery).
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSystemStatus, type Status } from "../system";
import { LineChart, Sparkline } from "./Chart";
import { AppIcon, VolumeSlider } from "./device";
import { bytes, Ring, usePoll } from "./gadgets";
import { formatRate, updateConfig, useStats, useTick, type WidgetProps } from "./shared";

const G = {
  bolt: "\uE945",
  lang: "\uF2B7",
  temp: "\uE9CA",
  sun: "\uE706",
  bell: "\uEA8F",
  keyboard: "\uE765",
  play: "\uE768",
  pause: "\uE769",
  next: "\uE893",
  prev: "\uE892",
  music: "\uEC4F",
  apps: "\uE71D",
  recent: "\uE81C",
  search: "\uE721",
  command: "\uE756",
  camera: "\uE722",
  photo: "\uE91B",
  folder: "\uE8B7",
  doc: "\uE8A5",
  down: "\uE74B",
  up: "\uE74A",
};

const stop = (e: React.SyntheticEvent) => e.stopPropagation();
const settingsPage = (page: string) => invoke("open_ms_settings", { page }).catch(console.error);
const pct = (v: number) => `${Math.round(v)}%`;
const DEMO_CPU = Array.from({ length: 60 }, (_, i) => 22 + 14 * Math.sin(i / 6) + 6 * Math.sin(i / 2.3));

// ---- CPU, memory, network --------------------------------------------------------

function RingStat({ label, value, size }: { label: string; value: number; size: string }) {
  return (
    <div className={`w-ringstat ${size}`} title={`${label} ${pct(value)}`}>
      <span className="ringwrap">
        <Ring percent={value} size={size === "s" ? 30 : 28} />
        {size === "s" && <b className="inner">{Math.round(value)}</b>}
      </span>
      {size !== "s" && (
        <span className="text">
          <small>{label}</small>
          <b>{pct(value)}</b>
        </span>
      )}
    </div>
  );
}

export function CpuTile({ instance, preview }: WidgetProps) {
  const s = useStats(!preview);
  const v = preview ? 37 : (s[s.length - 1]?.cpu ?? 0);
  return <RingStat label="CPU" value={v} size={instance.size} />;
}

export function RamTile({ instance, preview }: WidgetProps) {
  const s = useStats(!preview);
  const last = s[s.length - 1];
  const v = preview ? 62 : (last?.ram ?? 0);
  if (instance.size === "l" && last) {
    return (
      <div className="w-ringstat l" title={`Memory ${pct(v)}`}>
        <Ring percent={v} size={28} />
        <span className="text">
          <small>RAM</small>
          <b>
            {last.ram_used_gb.toFixed(1)} / {last.ram_total_gb.toFixed(0)} GB
          </b>
        </span>
      </div>
    );
  }
  return <RingStat label="RAM" value={v} size={instance.size} />;
}

export function NetworkTile({ instance, preview }: WidgetProps) {
  const s = useStats(!preview);
  const down = preview ? DEMO_CPU.map((v) => v * 30_000) : s.map((x) => x.down);
  const up = preview ? DEMO_CPU.map((v) => v * 4_000) : s.map((x) => x.up);
  const d = down[down.length - 1] ?? 0;
  const u = up[up.length - 1] ?? 0;
  const max = Math.max(1, ...down, ...up);
  return (
    <div className={`w-net ${instance.size}`} title={`Down ${formatRate(d)}, up ${formatRate(u)}`}>
      <span className="rates">
        <small>
          <span className="glyph">{G.down}</span>
          {formatRate(d)}
        </small>
        {instance.size !== "s" && (
          <small>
            <span className="glyph">{G.up}</span>
            {formatRate(u)}
          </small>
        )}
      </span>
      {instance.size !== "s" && <Sparkline values={down} max={max} />}
    </div>
  );
}

export function NetworkPanel() {
  const samples = useStats(true);
  const last = samples[samples.length - 1];
  return (
    <div className="panel w-system-panel">
      <h3>
        Network <span className="now">{last ? `↓ ${formatRate(last.down)} · ↑ ${formatRate(last.up)}` : "–"}</span>
      </h3>
      <LineChart
        series={[
          { name: "Down", color: "var(--series-1)", values: samples.map((s) => s.down) },
          { name: "Up", color: "var(--series-2)", values: samples.map((s) => s.up) },
        ]}
        format={formatRate}
        height={110}
      />
      <p className="fine">The last minute, every second, across all network adapters.</p>
    </div>
  );
}

// ---- Batteries ------------------------------------------------------------------------

const DEMO_STATUS: Partial<Status> = { battery: { percent: 76, charging: true, minutes_left: null } };

function useBattery(preview?: boolean) {
  const live = useSystemStatus();
  return preview ? DEMO_STATUS.battery! : live?.battery;
}

const batLevel = (p: number, charging: boolean) => (charging ? "charging" : p <= 15 ? "critical" : p <= 30 ? "warning" : "ok");

function NoBattery() {
  return <div className="w-battery none">No battery</div>;
}

export function BatteryRingTile({ instance, preview }: WidgetProps) {
  const b = useBattery(preview);
  if (!b) return <NoBattery />;
  const r = 12;
  const c = 2 * Math.PI * r;
  return (
    <div className={`w-batring ${instance.size} ${batLevel(b.percent, b.charging)}`} title={`Battery ${b.percent}%`}>
      <svg width="30" height="30" viewBox="0 0 30 30" aria-hidden>
        <circle cx="15" cy="15" r={r} className="track" />
        <circle cx="15" cy="15" r={r} className="fill" strokeDasharray={`${(c * b.percent) / 100} ${c}`} transform="rotate(-90 15 15)" />
        {b.charging && <circle cx="15" cy="15" r={r} className="pulse" />}
      </svg>
      <b className="inner">{b.charging ? <span className="glyph">{G.bolt}</span> : b.percent}</b>
      {instance.size !== "s" && (
        <span className="text">
          <b>{b.percent}%</b>
          <small>{b.charging ? "Charging" : b.minutes_left != null ? `${Math.floor(b.minutes_left / 60)}h ${b.minutes_left % 60}m` : "On battery"}</small>
        </span>
      )}
    </div>
  );
}

export function BatteryCoreTile({ instance, preview }: WidgetProps) {
  const b = useBattery(preview);
  if (!b) return <NoBattery />;
  const lit = Math.round((b.percent / 100) * 12);
  return (
    <div className={`w-batcore ${instance.size} ${batLevel(b.percent, b.charging)}`} title={`Battery ${b.percent}%`}>
      <svg width="30" height="30" viewBox="-15 -15 30 30" aria-hidden>
        {Array.from({ length: 12 }, (_, i) => (
          <rect
            key={i}
            x="-1.6"
            y="-13.5"
            width="3.2"
            height="6"
            rx="1.2"
            transform={`rotate(${i * 30})`}
            className={i < lit ? "on" : "off"}
            style={b.charging ? { animationDelay: `${i * 0.08}s` } : undefined}
          />
        ))}
        <circle r="4.5" className="core" />
      </svg>
      {instance.size !== "s" ? (
        <span className="text">
          <b>{b.percent}%</b>
          <small>{b.charging ? "Charging" : "Battery"}</small>
        </span>
      ) : null}
    </div>
  );
}

export function BatteryDotsTile({ instance, preview }: WidgetProps) {
  const b = useBattery(preview);
  if (!b) return <NoBattery />;
  const cols = instance.size === "s" ? 6 : 10;
  const lit = Math.round((b.percent / 100) * cols * 3);
  return (
    <div className={`w-batdots ${instance.size} ${batLevel(b.percent, b.charging)}`} title={`Battery ${b.percent}%`}>
      <span className="dots" style={{ gridTemplateColumns: `repeat(${cols}, 1fr)` }}>
        {Array.from({ length: cols * 3 }, (_, i) => {
          // Fill column by column, bottom to top, left to right.
          const col = i % cols;
          const row = 2 - Math.floor(i / cols);
          const order = col * 3 + row;
          return <i key={i} className={order < lit ? "on" : ""} />;
        })}
      </span>
      {instance.size !== "s" && <b>{b.percent}%</b>}
    </div>
  );
}

export function BatteryGaugeTile({ instance, preview }: WidgetProps) {
  const b = useBattery(preview);
  if (!b) return <NoBattery />;
  const a = (-120 + (b.percent / 100) * 240) * (Math.PI / 180);
  const arc = (from: number, to: number) => {
    const p = (deg: number) => {
      const r = (deg * Math.PI) / 180;
      return `${17 + Math.sin(r) * 13} ${19 - Math.cos(r) * 13}`;
    };
    return `M ${p(from)} A 13 13 0 ${to - from > 180 ? 1 : 0} 1 ${p(to)}`;
  };
  return (
    <div className={`w-batgauge ${instance.size} ${batLevel(b.percent, b.charging)}`} title={`Battery ${b.percent}%`}>
      <svg width="34" height="30" viewBox="0 0 34 30" aria-hidden>
        <path d={arc(-120, 120)} className="track" />
        <path d={arc(-120, -80)} className="low" />
        <line x1="17" y1="19" x2={17 + Math.sin(a) * 11} y2={19 - Math.cos(a) * 11} className="needle" />
        <circle cx="17" cy="19" r="2" className="hub" />
        <text x="5" y="29" className="ef">
          E
        </text>
        <text x="26" y="29" className="ef">
          F
        </text>
      </svg>
      {instance.size !== "s" && (
        <span className="text">
          <b>{b.percent}%</b>
          <small>{b.charging ? "Charging" : "Battery"}</small>
        </span>
      )}
    </div>
  );
}

// ---- Top CPU / top memory --------------------------------------------------------------------

type AppLoad = { name: string; exe: string; cpu: number; memory: number; processes: number };
type TopApps = { by_cpu: AppLoad[]; by_memory: AppLoad[]; uptime: number };
const DEMO_TOP: TopApps = {
  by_cpu: [
    { name: "Browser", exe: "", cpu: 12.4, memory: 1.8e9, processes: 14 },
    { name: "Editor", exe: "", cpu: 4.1, memory: 9e8, processes: 3 },
    { name: "Music", exe: "", cpu: 1.2, memory: 3e8, processes: 2 },
  ],
  by_memory: [
    { name: "Browser", exe: "", cpu: 12.4, memory: 1.8e9, processes: 14 },
    { name: "Editor", exe: "", cpu: 4.1, memory: 9e8, processes: 3 },
    { name: "Music", exe: "", cpu: 1.2, memory: 3e8, processes: 2 },
  ],
  uptime: 3600,
};
const loadPct = (v: number) => `${v < 10 ? v.toFixed(1) : Math.round(v)}%`;
// The app's own icon; without one (previews), its initial in a rounded square.
const AppGlyph = ({ exe, name }: { exe: string; name: string }) =>
  exe ? <AppIcon exe={exe} /> : <span className="letter">{name.slice(0, 1)}</span>;

function TopTile({ instance, preview, by }: WidgetProps & { by: "cpu" | "memory" }) {
  const [t] = usePoll<TopApps>("top_apps", 3000, DEMO_TOP, preview);
  const rows = (by === "cpu" ? t?.by_cpu : t?.by_memory) ?? [];
  const n = { s: 1, m: 2, l: 3 }[instance.size];
  const show = (a: AppLoad) => (by === "cpu" ? loadPct(a.cpu) : bytes(a.memory));
  return (
    <div
      className={`w-top3 ${instance.size}`}
      title={rows.slice(0, 3).map((a) => `${a.name}: ${show(a)}`).join("\n")}
      onDoubleClick={() => !preview && invoke("open_task_manager")}
    >
      <small className="lbl">{by === "cpu" ? "CPU" : "RAM"}</small>
      {rows.slice(0, n).map((a) => (
        <span key={a.exe || a.name} className="app">
          <AppGlyph exe={a.exe} name={a.name} />
          <b>{show(a)}</b>
        </span>
      ))}
    </div>
  );
}

export const TopCpuTile = (p: WidgetProps) => <TopTile {...p} by="cpu" />;
export const TopRamTile = (p: WidgetProps) => <TopTile {...p} by="memory" />;

// ---- Day / month / year progress ------------------------------------------------------------

function span(unit: "day" | "month" | "year", now: Date) {
  const y = now.getFullYear();
  const m = now.getMonth();
  const [a, b] =
    unit === "day"
      ? [new Date(y, m, now.getDate()), new Date(y, m, now.getDate() + 1)]
      : unit === "month"
        ? [new Date(y, m, 1), new Date(y, m + 1, 1)]
        : [new Date(y, 0, 1), new Date(y + 1, 0, 1)];
  return { f: (now.getTime() - a.getTime()) / (b.getTime() - a.getTime()), a, b };
}

function UnitTile({ instance, unit }: WidgetProps & { unit: "day" | "month" }) {
  const now = new Date(useTick(60_000));
  const { f } = span(unit, now);
  const label = unit === "day" ? "Today" : now.toLocaleDateString([], { month: "long" });
  return (
    <div className={`w-unit ${instance.size}`} title={`${(f * 100).toFixed(1)}% of ${unit === "day" ? "today" : "this month"} has passed`}>
      <Ring percent={f * 100} size={26} />
      {instance.size === "s" ? (
        <b>{Math.floor(f * 100)}%</b>
      ) : (
        <span className="text">
          <small>{label}</small>
          <b>{Math.floor(f * 100)}%</b>
        </span>
      )}
      {instance.size === "l" && (
        <small className="left">
          {unit === "day"
            ? `${23 - now.getHours()}h ${59 - now.getMinutes()}m left`
            : `${new Date(now.getFullYear(), now.getMonth() + 1, 0).getDate() - now.getDate()} days left`}
        </small>
      )}
    </div>
  );
}

export const DayProgressTile = (p: WidgetProps) => <UnitTile {...p} unit="day" />;
export const MonthProgressTile = (p: WidgetProps) => <UnitTile {...p} unit="month" />;

export function YearProgressTile({ instance }: WidgetProps) {
  const now = new Date(useTick(60_000));
  const { f, a, b } = span("year", now);
  const days = Math.round((b.getTime() - a.getTime()) / 864e5);
  const done = Math.floor(f * days);
  // One dot per week in the tile (53 is too many to see), one per day in the details.
  const cols = { s: 9, m: 18, l: 27 }[instance.size];
  const dots = cols * 3;
  const lit = Math.round(f * dots);
  return (
    <div className={`w-year ${instance.size}`} title={`Day ${done + 1} of ${days}, ${(f * 100).toFixed(1)}%`}>
      <span className="dots" style={{ gridTemplateColumns: `repeat(${cols}, 1fr)` }}>
        {Array.from({ length: dots }, (_, i) => (
          <i key={i} className={i < lit ? "on" : i === lit ? "now" : ""} />
        ))}
      </span>
      {instance.size !== "s" && <b>{Math.floor(f * 100)}%</b>}
    </div>
  );
}

export function YearProgressPanel() {
  const now = new Date(useTick(60_000));
  const { f, a, b } = span("year", now);
  const days = Math.round((b.getTime() - a.getTime()) / 864e5);
  const today = Math.floor(f * days);
  return (
    <div className="panel w-year-panel">
      <h3>
        {now.getFullYear()} <span className="now">{(f * 100).toFixed(1)}% · {days - today - 1} days left</span>
      </h3>
      <div className="yeargrid">
        {Array.from({ length: days }, (_, i) => (
          <i key={i} className={i < today ? "on" : i === today ? "now" : ""} title={new Date(a.getTime() + i * 864e5 + 3600e3).toLocaleDateString()} />
        ))}
      </div>
      <p className="fine">One dot per day. Today is outlined.</p>
    </div>
  );
}

// ---- Language -------------------------------------------------------------------------------

type Lang = { short: string; name: string; all: string[] };

export function LanguageTile({ instance, preview }: WidgetProps) {
  const [l, , load] = usePoll<Lang>("language_get", 2000, { short: "ENG", name: "English (United States)", all: [] }, preview);
  return (
    <button
      className={`w-lang ${instance.size}`}
      title={`${l?.name ?? ""}\nClick to switch (Win+Space)`}
      onClick={(e) => {
        stop(e);
        if (preview) return;
        invoke("language_next").then(() => setTimeout(load, 600));
      }}
    >
      <b>{l?.short ?? "…"}</b>
      {instance.size !== "s" && <small>{l?.name.replace(/ \(.*\)$/, "")}</small>}
    </button>
  );
}

export function LanguagePanel() {
  const [l, , load] = usePoll<Lang>("language_get", 2000, { short: "", name: "", all: [] });
  return (
    <div className="panel">
      <h3>
        Keyboard <span className="now">{l?.name}</span>
      </h3>
      <ul className="plain">
        {l?.all.map((n) => (
          <li key={n} className={n === l.name ? "on" : ""}>
            {n}
          </li>
        ))}
      </ul>
      <div className="actions">
        <button onClick={() => invoke("language_next").then(() => setTimeout(load, 600))}>Next layout</button>
        <button className="link" onClick={() => settingsPage("regionlanguage")}>
          Language settings
        </button>
      </div>
      <p className="fine">Shows the layout of the window you're typing in. Switching works like Win+Space.</p>
    </div>
  );
}

// ---- Temperature -------------------------------------------------------------------------------

type Temp = { celsius: number | null; zones: [string, number][] };

export function TemperatureTile({ instance, preview }: WidgetProps<{ fahrenheit?: boolean }>) {
  const [t] = usePoll<Temp>("temperature", 5000, { celsius: 54, zones: [] }, preview);
  const s = useStats(!preview && instance.size === "l");
  const c = t?.celsius;
  const f = instance.config.fahrenheit;
  const shown = c == null ? "–" : f ? `${Math.round(c * 1.8 + 32)}°` : `${Math.round(c)}°`;
  const hot = c != null && c >= 85 ? "critical" : c != null && c >= 70 ? "warning" : "ok";
  return (
    <div className={`w-temp ${instance.size} ${hot}`} title={c == null ? "This PC doesn't report a temperature" : `${shown} (hottest sensor)`}>
      <Ring percent={c == null ? 0 : Math.min(100, (c / 100) * 100)} size={26} />
      <b>{shown}</b>
      {instance.size === "l" && (
        <small>
          CPU {pct(preview ? 23 : (s[s.length - 1]?.cpu ?? 0))} · RAM {pct(preview ? 61 : (s[s.length - 1]?.ram ?? 0))}
        </small>
      )}
    </div>
  );
}

export function TemperaturePanel({ instance }: WidgetProps<{ fahrenheit?: boolean }>) {
  const [t] = usePoll<Temp>("temperature", 3000, { celsius: null, zones: [] });
  const s = useStats(true);
  const last = s[s.length - 1];
  const f = instance.config.fahrenheit;
  const show = (c: number) => (f ? `${Math.round(c * 1.8 + 32)}°F` : `${c.toFixed(1)}°C`);
  return (
    <div className="panel w-ai-panel">
      <h3>Temperature</h3>
      {t?.celsius == null ? (
        <p className="fine">This PC doesn't report a temperature to Windows. Many desktops don't; most laptops do.</p>
      ) : (
        <div className="reading">
          <div className="headline">
            <b>{show(t.celsius)}</b>
            <span>Hottest sensor</span>
          </div>
          {t.zones.map(([name, c]) => (
            <div key={name} className="line">
              <span>{name}</span>
              <b>{show(c)}</b>
            </div>
          ))}
          {last && (
            <>
              <div className="line">
                <span>CPU load</span>
                <b>{pct(last.cpu)}</b>
              </div>
              <div className="line">
                <span>Memory in use</span>
                <b>{pct(last.ram)}</b>
              </div>
            </>
          )}
        </div>
      )}
      <div className="actions">
        <span className="fine">From the board's thermal sensors, as Windows reports them</span>
        <button className="link" onClick={() => updateConfig(instance.id, { ...instance.config, fahrenheit: !f })}>
          Show °{f ? "C" : "F"}
        </button>
      </div>
    </div>
  );
}

// ---- Brightness ----------------------------------------------------------------------------

function useBrightness(preview?: boolean) {
  const [level, setLevel] = useState<number | null | undefined>(preview ? 70 : undefined);
  const [local, setLocal] = useState<number | null>(null);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => {
    if (preview) return;
    const load = () => invoke<number | null>("brightness_get").then(setLevel);
    load();
    const id = setInterval(load, 30_000);
    return () => clearInterval(id);
  }, [preview]);
  const set = (v: number) => {
    setLocal(v);
    if (preview) return;
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      invoke("brightness_set", { level: v }).then(() => setLevel(v)).finally(() => setLocal(null));
    }, 150);
  };
  return { level: local ?? level, set };
}

export function BrightnessTile({ instance, preview }: WidgetProps) {
  const { level, set } = useBrightness(preview);
  if (level === null) return <div className="w-bright none" title="This screen's brightness can't be set from Windows">No control</div>;
  return (
    <div className={`w-bright ${instance.size}`}>
      <span className="glyph">{G.sun}</span>
      {instance.size === "s" ? <b>{level ?? "…"}</b> : <VolumeSlider level={level ?? 0} onChange={set} label="Brightness" />}
      {instance.size === "l" && <b>{level ?? ""}</b>}
    </div>
  );
}

export function BrightnessPanel() {
  const { level, set } = useBrightness();
  return (
    <div className="panel">
      <h3>Brightness</h3>
      {level === null ? (
        <p className="fine">This screen's brightness can't be set from Windows. External monitors usually have their own buttons.</p>
      ) : (
        <>
          <div className="big-number">
            {level ?? "…"}
            <span>%</span>
          </div>
          <VolumeSlider level={level ?? 0} onChange={set} label="Brightness" />
        </>
      )}
      <div className="actions">
        <span />
        <button className="link" onClick={() => settingsPage("display")}>
          Display settings
        </button>
      </div>
    </div>
  );
}

// ---- Notifications and keyboard -------------------------------------------------------------

export function NotificationsTile({ instance, preview }: WidgetProps) {
  return (
    <button
      className={`w-action ${instance.size}`}
      title="Notifications (Win+N)"
      onClick={(e) => {
        stop(e);
        if (!preview) invoke("open_flyout", { which: "notifications" });
      }}
    >
      <span className="glyph">{G.bell}</span>
      {instance.size !== "s" && <span>Notifications</span>}
    </button>
  );
}

export function NotificationsPanel() {
  return (
    <div className="panel">
      <h3>Notifications</h3>
      <p className="fine">Click the tile to open the notification center, where Do not disturb is too. Win+N does the same.</p>
      <div className="actions">
        <button onClick={() => invoke("open_flyout", { which: "notifications" })}>Open</button>
        <button className="link" onClick={() => settingsPage("notifications")}>
          Notification settings
        </button>
      </div>
    </div>
  );
}

export function KeyboardTile({ instance, preview }: WidgetProps) {
  return (
    <button
      className={`w-action ${instance.size}`}
      title="On-screen keyboard"
      onClick={(e) => {
        stop(e);
        if (!preview) invoke("open_on_screen_keyboard");
      }}
    >
      <span className="glyph">{G.keyboard}</span>
      {instance.size !== "s" && <span>Keyboard</span>}
    </button>
  );
}

export function KeyboardPanel() {
  return (
    <div className="panel">
      <h3>On-screen keyboard</h3>
      <p className="fine">Click the tile to open the touch keyboard (or the classic on-screen keyboard on PCs without one).</p>
      <div className="actions">
        <button onClick={() => invoke("open_on_screen_keyboard")}>Open</button>
        <button className="link" onClick={() => settingsPage("typing")}>
          Typing settings
        </button>
      </div>
    </div>
  );
}

// ---- Now playing, compact looks -------------------------------------------------------------

type Media = NonNullable<Status["media"]>;
const DEMO_MEDIA: Media = { title: "Night Drive", artist: "Neon Coast", playing: true, art: null };
const control = (action: string) => invoke("media_control", { action }).catch(console.error);

function useMedia(preview?: boolean) {
  const live = useSystemStatus();
  return preview ? DEMO_MEDIA : (live?.media ?? null);
}

function Btn({ glyph, label, action, preview, className = "" }: { glyph: string; label: string; action: string; preview?: boolean; className?: string }) {
  return (
    <button
      className={`glyph ${className}`}
      aria-label={label}
      title={label}
      onClick={(e) => {
        stop(e);
        if (!preview) control(action);
      }}
    >
      {glyph}
    </button>
  );
}

/** Track progress isn't reported by every app; the ring and bar pulse while playing instead. */
export function NowCompactTile({ instance, preview }: WidgetProps) {
  const m = useMedia(preview);
  return (
    <div className={`w-nowc ${instance.size}${m?.playing ? " playing" : ""}`} title={m ? `${m.title}${m.artist ? ` — ${m.artist}` : ""}` : "Nothing playing"}>
      <Btn glyph={m?.playing ? G.pause : G.play} label={m?.playing ? "Pause" : "Play"} action="toggle" preview={preview} className="pp" />
      <span className="text">
        <b>{m?.title ?? "Nothing playing"}</b>
        {instance.size !== "s" && m?.artist && <small>{m.artist}</small>}
      </span>
    </div>
  );
}

export function NowMiniTile({ instance, preview }: WidgetProps) {
  const m = useMedia(preview);
  return (
    <div className={`w-nowmini ${instance.size}${m?.playing ? " playing" : ""}`} title={m ? m.title : "Nothing playing"}>
      <span className="orb">
        {m?.art ? <img src={m.art} alt="" draggable={false} /> : null}
        <Btn glyph={m?.playing ? G.pause : G.play} label={m?.playing ? "Pause" : "Play"} action="toggle" preview={preview} className="pp" />
      </span>
      {instance.size !== "s" && (
        <span className="text">
          <b>{m?.title ?? "Nothing playing"}</b>
          {instance.size === "l" && m?.artist && <small>{m.artist}</small>}
        </span>
      )}
    </div>
  );
}

export function NowBarTile({ instance, preview }: WidgetProps) {
  const m = useMedia(preview);
  return (
    <div className={`w-nowbar ${instance.size}${m?.playing ? " playing" : ""}`} title={m ? m.title : "Nothing playing"}>
      <span className="col">
        <span className="row">
          <b>{m?.title ?? "Nothing playing"}</b>
          {instance.size !== "s" && <Btn glyph={G.prev} label="Previous" action="previous" preview={preview} />}
          <Btn glyph={m?.playing ? G.pause : G.play} label={m?.playing ? "Pause" : "Play"} action="toggle" preview={preview} />
          <Btn glyph={G.next} label="Next" action="next" preview={preview} />
        </span>
        <span className="bar">
          <i />
        </span>
      </span>
    </div>
  );
}

// ---- Apps & files --------------------------------------------------------------------------

type Shortcut = { name: string; path: string; kind: string; ago: number };

function ShortcutIcon({ s }: { s: Shortcut }) {
  if (s.kind === "app") return <AppIcon exe={s.path} />;
  return <span className="glyph">{s.kind === "folder" ? G.folder : ["png", "jpg", "jpeg", "gif", "webp", "heic"].includes(s.kind) ? G.photo : G.doc}</span>;
}

const ago = (secs: number) =>
  secs < 3600 ? `${Math.max(1, Math.round(secs / 60))}m ago` : secs < 86400 ? `${Math.round(secs / 3600)}h ago` : `${Math.round(secs / 86400)}d ago`;

export function AppsTile({ instance, preview }: WidgetProps) {
  return (
    <button
      className={`w-action ${instance.size}`}
      title="Start"
      onClick={(e) => {
        stop(e);
        if (!preview) invoke("open_flyout", { which: "start" });
      }}
    >
      <span className="glyph">{G.apps}</span>
      {instance.size !== "s" && <span>Apps</span>}
    </button>
  );
}

function AppSearch({ autoFocus }: { autoFocus?: boolean }) {
  const [all, setAll] = useState<Shortcut[]>([]);
  const [q, setQ] = useState("");
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    invoke<Shortcut[]>("start_apps").then(setAll);
  }, []);
  const words = q.trim().toLowerCase();
  const matches = useMemo(() => {
    if (!words) return [];
    const starts = all.filter((a) => a.name.toLowerCase().startsWith(words));
    const has = all.filter((a) => !a.name.toLowerCase().startsWith(words) && a.name.toLowerCase().includes(words));
    return [...starts, ...has].slice(0, 7);
  }, [all, words]);
  const actions = ACTIONS.filter((a) => words && a.words.some((w) => w.startsWith(words) || words.startsWith(w))).slice(0, 3);
  const open = (path: string) => invoke("open_shortcut", { path }).then(() => setQ("")).catch((e) => setError(String(e)));
  return (
    <>
      <input
        autoFocus={autoFocus}
        placeholder="Type an app or a command (lock, downloads…)"
        value={q}
        onChange={(e) => {
          setQ(e.target.value);
          setError(null);
        }}
        onKeyDown={(e) => {
          if (e.key !== "Enter") return;
          if (matches[0]) open(matches[0].path);
          else if (actions[0]) invoke("run_action", { action: actions[0].id }).then(() => setQ(""));
        }}
      />
      {error && <p className="error">{error}</p>}
      <ul className="launch">
        {actions.map((a) => (
          <li key={a.id}>
            <button onClick={() => invoke("run_action", { action: a.id }).then(() => setQ(""))}>
              <span className="glyph">{G.command}</span>
              <span className="name">{a.label}</span>
            </button>
          </li>
        ))}
        {matches.map((s) => (
          <li key={s.path}>
            <button onClick={() => open(s.path)}>
              <ShortcutIcon s={s} />
              <span className="name">{s.name}</span>
            </button>
          </li>
        ))}
      </ul>
    </>
  );
}

const ACTIONS = [
  { id: "lock", label: "Lock the PC", words: ["lock"] },
  { id: "settings", label: "Settings", words: ["settings"] },
  { id: "explorer", label: "File Explorer", words: ["explorer", "files"] },
  { id: "taskmgr", label: "Task Manager", words: ["task", "taskmgr"] },
  { id: "terminal", label: "Terminal", words: ["terminal", "cmd", "shell"] },
  { id: "downloads", label: "Downloads folder", words: ["downloads"] },
  { id: "documents", label: "Documents folder", words: ["documents", "docs"] },
  { id: "screenshots", label: "Screenshots folder", words: ["screenshots"] },
  { id: "bin", label: "Recycle Bin", words: ["bin", "recycle", "trash"] },
  { id: "controlpanel", label: "Control Panel", words: ["control"] },
  { id: "calc", label: "Calculator", words: ["calc", "calculator"] },
  { id: "notepad", label: "Notepad", words: ["notepad", "note"] },
];

export function AppsPanel() {
  const [apps] = usePoll<{ key: string; name: string; icon: string; windows: { hwnd: number; title: string }[] }[]>("apps_snapshot", 3000, []);
  const open = (apps ?? []).filter((a) => a.windows.length);
  return (
    <div className="panel w-apps-panel">
      <h3>Apps</h3>
      <AppSearch autoFocus />
      <h4>Open windows</h4>
      <ul className="launch">
        {open.flatMap((a) =>
          a.windows.map((w) => (
            <li key={w.hwnd}>
              <button onClick={() => invoke("window_focus", { hwndValue: w.hwnd })}>
                <AppIcon exe={a.icon} />
                <span className="name">{w.title || a.name}</span>
              </button>
            </li>
          )),
        )}
      </ul>
      <div className="actions">
        <span />
        <button className="link" onClick={() => invoke("open_flyout", { which: "start" })}>
          Open Start
        </button>
      </div>
    </div>
  );
}

export function CommandsTile({ instance, preview }: WidgetProps) {
  return (
    <div className={`w-action ${instance.size}`} title="Commands: open to type an app or action">
      <span className="glyph">{G.command}</span>
      {instance.size !== "s" && <span>{preview ? "Run…" : "Run…"}</span>}
    </div>
  );
}

export function CommandsPanel() {
  return (
    <div className="panel w-apps-panel">
      <h3>Commands</h3>
      <AppSearch autoFocus />
      <p className="fine">Type part of an app's name and press Enter, or a word like lock, downloads, terminal or calc.</p>
    </div>
  );
}

export function RecentTile({ instance, preview }: WidgetProps) {
  const [r] = usePoll<Shortcut[]>("recent_files", 30_000, [{ name: "Report.docx", path: "", kind: "docx", ago: 300 }], preview);
  const first = r?.[0];
  return (
    <div className={`w-action ${instance.size}`} title={first ? `Last opened: ${first.name}` : "Recently opened"}>
      <span className="glyph">{G.recent}</span>
      {instance.size !== "s" && <span className="first">{first?.name ?? "Recent"}</span>}
    </div>
  );
}

export function RecentPanel() {
  const [r] = usePoll<Shortcut[]>("recent_files", 15_000, []);
  return (
    <div className="panel w-apps-panel">
      <h3>Recently opened</h3>
      {!r?.length && <p className="fine">Nothing yet. Files and folders you open show up here.</p>}
      <ul className="launch">
        {r?.map((s) => (
          <li key={s.path}>
            <button onClick={() => invoke("open_shortcut", { path: s.path })}>
              <ShortcutIcon s={s} />
              <span className="name">{s.name}</span>
              <small>{ago(s.ago)}</small>
            </button>
          </li>
        ))}
      </ul>
      <p className="fine">From Windows' own Recent list, on this PC.</p>
    </div>
  );
}

type SearchConfig = { engine?: "google" | "bing" | "duckduckgo" };

export function SearchTile({ instance }: WidgetProps<SearchConfig>) {
  return (
    <div className={`w-action search ${instance.size}`} title="Search the web or open a folder">
      <span className="glyph">{G.search}</span>
      {instance.size !== "s" && <span className="ph">Search…</span>}
    </div>
  );
}

export function SearchPanel({ instance }: WidgetProps<SearchConfig>) {
  const [q, setQ] = useState("");
  const [error, setError] = useState<string | null>(null);
  const engine = instance.config.engine ?? "google";
  return (
    <div className="panel">
      <h3>Search</h3>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          invoke("search_open", { query: q, engine })
            .then(() => setQ(""))
            .catch((err) => setError(String(err)));
        }}
      >
        <input autoFocus placeholder="Search the web, or a path like C:\Users" value={q} onChange={(e) => (setQ(e.target.value), setError(null))} />
      </form>
      {error && <p className="error">{error}</p>}
      <div className="seg" role="radiogroup" aria-label="Search with">
        {(["google", "bing", "duckduckgo"] as const).map((x) => (
          <button key={x} className={engine === x ? "on" : ""} onClick={() => updateConfig(instance.id, { ...instance.config, engine: x })}>
            {x === "duckduckgo" ? "DuckDuckGo" : x[0].toUpperCase() + x.slice(1)}
          </button>
        ))}
      </div>
      <p className="fine">Press Enter. A folder or file path opens in File Explorer; anything else opens a web search in your browser.</p>
    </div>
  );
}

export function ScreenshotTile({ instance, preview }: WidgetProps) {
  return (
    <button
      className={`w-action ${instance.size}`}
      title="Snip a region (more in the details)"
      onClick={(e) => {
        stop(e);
        if (!preview) invoke("screenshot", { mode: "region" });
      }}
    >
      <span className="glyph">{G.camera}</span>
      {instance.size !== "s" && <span>Snip</span>}
    </button>
  );
}

export function ScreenshotPanel() {
  const shot = (mode: string) => invoke("screenshot", { mode });
  return (
    <div className="panel">
      <h3>Screenshot</h3>
      <div className="shot-buttons">
        <button onClick={() => shot("screen")}>Full screen</button>
        <button onClick={() => shot("window")}>A window</button>
        <button onClick={() => shot("region")}>A region</button>
      </div>
      <p className="fine">Full screen saves straight to Pictures\Screenshots. Window and region open Windows' snipping overlay, which copies the shot to the clipboard.</p>
      <div className="actions">
        <span />
        <button className="link" onClick={() => shot("folder")}>
          Open Screenshots folder
        </button>
      </div>
    </div>
  );
}

// ---- Wallpaper gallery ------------------------------------------------------------------------

type Picture = { id: string; title: string; credit: string; thumb: string };

export function GalleryTile({ instance, preview }: WidgetProps) {
  const [current] = usePoll<string | null>("wallpaper_image", 120_000, null, preview);
  return (
    <div className={`w-gallery ${instance.size}`} title="Wallpaper gallery">
      {current ? <img src={current} alt="" draggable={false} /> : <span className="glyph">{G.photo}</span>}
      {instance.size !== "s" && <span>Gallery</span>}
    </div>
  );
}

export function GalleryPanel() {
  const [pics, setPics] = useState<Picture[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  useEffect(() => {
    invoke<Picture[]>("wallpaper_gallery").then(setPics).catch((e) => setError(String(e)));
  }, []);
  return (
    <div className="panel w-gallery-panel">
      <h3>Wallpaper gallery</h3>
      {error && <p className="error">{error}</p>}
      {!pics && !error && <p className="fine">Loading this week's pictures…</p>}
      <div className="pics">
        {pics?.map((p) => (
          <button
            key={p.id}
            className={busy === p.id ? "busy" : ""}
            title={`${p.title}\n${p.credit}`}
            disabled={!!busy}
            onClick={() => {
              setBusy(p.id);
              setError(null);
              invoke("wallpaper_set_gallery", { id: p.id })
                .catch((e) => setError(String(e)))
                .finally(() => setBusy(null));
            }}
          >
            <img src={p.thumb} alt={p.title} draggable={false} />
          </button>
        ))}
      </div>
      <p className="fine">Bing's picture of the day for the last week. Click one to make it your desktop background; it's saved on this PC.</p>
    </div>
  );
}
