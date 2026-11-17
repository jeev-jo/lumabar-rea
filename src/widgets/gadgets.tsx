/**
 * System gadgets: top apps by CPU and memory, storage, Recycle Bin, microphone,
 * light/dark theme, Show desktop, and day/month/year progress.
 */
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AppIcon, VolumeSlider } from "./device";
import { useTick, type WidgetProps } from "./shared";

const G = {
  apps: "",
  drive: "",
  bin: "",
  mic: "",
  micOff: "",
  sun: "",
  moon: "",
  desktop: "",
};

/** Controls inside a tile must not also open the tile's details. */
const stop = (e: React.SyntheticEvent) => e.stopPropagation();
const settingsPage = (page: string) => invoke("open_ms_settings", { page }).catch(console.error);

function bytes(n: number) {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return `${n >= 100 || i === 0 ? Math.round(n) : n.toFixed(1)} ${units[i]}`;
}

/** Ask Rust for a value now and every `ms` while the widget is shown. */
function usePoll<T>(command: string, ms: number, demo: T, preview?: boolean) {
  const [value, setValue] = useState<T | null>(preview ? demo : null);
  const load = useCallback(() => {
    if (!preview) invoke<T>(command).then(setValue).catch(() => {});
  }, [command, preview]);
  useEffect(() => {
    if (preview) return;
    load();
    const id = setInterval(load, ms);
    return () => clearInterval(id);
  }, [load, ms, preview]);
  return [value, setValue, load] as const;
}

// ---- Top apps -------------------------------------------------------------------

type AppLoad = { name: string; exe: string; cpu: number; memory: number; processes: number };
type TopApps = { by_cpu: AppLoad[]; by_memory: AppLoad[]; uptime: number };

const DEMO_TOP: TopApps = {
  by_cpu: [{ name: "Browser", exe: "", cpu: 12.4, memory: 1.8e9, processes: 14 }],
  by_memory: [{ name: "Browser", exe: "", cpu: 12.4, memory: 1.8e9, processes: 14 }],
  uptime: 3 * 3600,
};

const pct = (v: number) => `${v < 10 ? v.toFixed(1) : Math.round(v)}%`;

function Icon({ exe }: { exe: string }) {
  return exe ? <AppIcon exe={exe} /> : <span className="glyph">{G.apps}</span>;
}

export function TopAppsTile({ instance, preview }: WidgetProps) {
  const [t] = usePoll<TopApps>("top_apps", 3000, DEMO_TOP, preview);
  const cpu = t?.by_cpu[0];
  const mem = t?.by_memory[0];
  if (!cpu) return <div className="w-top">…</div>;
  if (instance.size === "l" && mem) {
    return (
      <div className="w-top l" title={`Most CPU: ${cpu.name}\nMost memory: ${mem.name}`}>
        <span className="row">
          <small>CPU</small>
          <Icon exe={cpu.exe} />
          <b>{pct(cpu.cpu)}</b>
        </span>
        <span className="row">
          <small>RAM</small>
          <Icon exe={mem.exe} />
          <b>{bytes(mem.memory)}</b>
        </span>
      </div>
    );
  }
  return (
    <div className={`w-top ${instance.size}`} title={`Most CPU: ${cpu.name}, ${pct(cpu.cpu)}`}>
      <Icon exe={cpu.exe} />
      {instance.size === "m" && <span className="name">{cpu.name}</span>}
      <b>{pct(cpu.cpu)}</b>
    </div>
  );
}

function uptime(secs: number) {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return d ? `${d}d ${h}h` : h ? `${h}h ${m}m` : `${m}m`;
}

export function TopAppsPanel() {
  const [t] = usePoll<TopApps>("top_apps", 2000, DEMO_TOP);
  const [tab, setTab] = useState<"cpu" | "memory">("cpu");
  const rows = tab === "cpu" ? t?.by_cpu : t?.by_memory;
  return (
    <div className="panel w-top-panel">
      <h3>
        Top apps
        {t && <span className="now">up {uptime(t.uptime)}</span>}
      </h3>
      <div className="seg" role="tablist">
        <button className={tab === "cpu" ? "on" : ""} onClick={() => setTab("cpu")}>
          CPU
        </button>
        <button className={tab === "memory" ? "on" : ""} onClick={() => setTab("memory")}>
          Memory
        </button>
      </div>
      <ul className="top-list">
        {rows?.map((a) => (
          <li key={a.exe || a.name}>
            <Icon exe={a.exe} />
            <span className="name">
              {a.name}
              {a.processes > 1 && <small> ({a.processes})</small>}
            </span>
            <b>{tab === "cpu" ? pct(a.cpu) : bytes(a.memory)}</b>
          </li>
        ))}
      </ul>
      <div className="actions">
        <span className="fine">Grouped by app, like Task Manager</span>
        <button className="link" onClick={() => invoke("open_task_manager")}>
          Task Manager
        </button>
      </div>
    </div>
  );
}

// ---- Storage --------------------------------------------------------------------

type Drive = { letter: string; label: string; total: number; free: number; removable: boolean };
const DEMO_DRIVES: Drive[] = [{ letter: "C:", label: "Windows", total: 512e9, free: 180e9, removable: false }];

const used = (d: Drive) => Math.round(((d.total - d.free) / d.total) * 100);
const level = (p: number) => (p >= 90 ? "critical" : p >= 75 ? "warning" : "ok");

function Ring({ percent, size = 24 }: { percent: number; size?: number }) {
  const r = (size - 4) / 2;
  const c = 2 * Math.PI * r;
  return (
    <svg className={`ring ${level(percent)}`} width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden>
      <circle cx={size / 2} cy={size / 2} r={r} className="track" />
      <circle
        cx={size / 2}
        cy={size / 2}
        r={r}
        className="fill"
        strokeDasharray={`${(c * Math.min(100, percent)) / 100} ${c}`}
        transform={`rotate(-90 ${size / 2} ${size / 2})`}
      />
    </svg>
  );
}

export function StorageTile({ instance, preview }: WidgetProps) {
  const [drives] = usePoll<Drive[]>("drives", 60_000, DEMO_DRIVES, preview);
  const d = drives?.find((x) => x.letter === "C:") ?? drives?.[0];
  if (!d) return <div className="w-storage">…</div>;
  return (
    <div className={`w-storage ${instance.size}`} title={`${d.letter} ${bytes(d.free)} free of ${bytes(d.total)}`}>
      <Ring percent={used(d)} />
      {instance.size === "s" ? (
        <b>{used(d)}%</b>
      ) : (
        <span className="text">
          <b>{bytes(d.free)}</b>
          <small>free on {d.letter}</small>
        </span>
      )}
    </div>
  );
}

export function StoragePanel() {
  const [drives] = usePoll<Drive[]>("drives", 30_000, DEMO_DRIVES);
  return (
    <div className="panel w-storage-panel">
      <h3>Storage</h3>
      {drives?.map((d) => (
        <button key={d.letter} className="drive" onClick={() => invoke("open_drive", { letter: d.letter })} title="Open in File Explorer">
          <span className="glyph">{G.drive}</span>
          <span className="info">
            <span className="top">
              <b>
                {d.label || (d.removable ? "USB drive" : "Local disk")} ({d.letter})
              </b>
              <small>
                {bytes(d.free)} free of {bytes(d.total)}
              </small>
            </span>
            <span className={`meter ${level(used(d))}`}>
              <i style={{ width: `${Math.max(2, used(d))}%` }} />
            </span>
          </span>
        </button>
      ))}
      <div className="actions">
        <span />
        <button className="link" onClick={() => settingsPage("storagesense")}>
          Storage settings
        </button>
      </div>
    </div>
  );
}

// ---- Recycle Bin ----------------------------------------------------------------

type Bin = { items: number; bytes: number };

export function RecycleBinTile({ instance, preview }: WidgetProps) {
  const [bin] = usePoll<Bin>("recycle_bin", 10_000, { items: 12, bytes: 48e6 }, preview);
  const n = bin?.items ?? 0;
  return (
    <div className={`w-bin ${instance.size}${n ? " full" : ""}`} title={n ? `${n} items, ${bytes(bin!.bytes)}` : "Recycle Bin is empty"}>
      <span className="glyph">{G.bin}</span>
      {instance.size === "m" ? (
        <span className="text">
          <b>{n ? n : "Empty"}</b>
          {n > 0 && <small>{bytes(bin!.bytes)}</small>}
        </span>
      ) : (
        n > 0 && <b>{n > 999 ? "999+" : n}</b>
      )}
    </div>
  );
}

export function RecycleBinPanel() {
  const [bin, , load] = usePoll<Bin>("recycle_bin", 3000, { items: 0, bytes: 0 });
  const n = bin?.items ?? 0;
  return (
    <div className="panel">
      <h3>Recycle Bin</h3>
      <div className="big-number">
        {n}
        <span>{n === 1 ? " item" : " items"}</span>
      </div>
      <p className="fine">{n ? `${bytes(bin!.bytes)} you can get back` : "Nothing in the Recycle Bin."}</p>
      <div className="actions">
        <button onClick={() => invoke("recycle_bin_open")}>Open</button>
        <button
          disabled={!n}
          onClick={() => {
            // Windows asks to confirm before anything is deleted.
            invoke("recycle_bin_empty").then(() => setTimeout(load, 1500));
          }}
        >
          Empty…
        </button>
      </div>
    </div>
  );
}

// ---- Microphone -----------------------------------------------------------------

type Mic = { level: number; muted: boolean; device: string };

function useMic(preview?: boolean, ms = 3000) {
  const [mic, setMic] = usePoll<Mic | null>("mic_status", ms, { level: 70, muted: false, device: "Microphone" }, preview);
  const toggle = () => !preview && invoke<Mic | null>("mic_toggle_mute").then(setMic).catch(console.error);
  const set = (level: number) => {
    if (preview || !mic) return;
    setMic({ ...mic, level, muted: false });
    invoke<Mic | null>("mic_set", { level }).catch(console.error);
    if (mic.muted) toggle();
  };
  return { mic, toggle, set };
}

export function MicTile({ instance, preview }: WidgetProps) {
  const { mic, toggle } = useMic(preview);
  if (!mic) return <div className="w-mic none">No mic</div>;
  return (
    <div className={`w-mic ${instance.size}${mic.muted ? " muted" : ""}`}>
      <button
        className="glyph toggle"
        title={mic.muted ? "Unmute microphone" : "Mute microphone"}
        onClick={(e) => {
          stop(e);
          toggle();
        }}
      >
        {mic.muted ? G.micOff : G.mic}
      </button>
      {instance.size === "m" && (
        <span className="text">
          <b>{mic.device}</b>
          <small>{mic.muted ? "Muted" : `${mic.level}%`}</small>
        </span>
      )}
    </div>
  );
}

export function MicPanel() {
  const { mic, toggle, set } = useMic(false, 2000);
  return (
    <div className="panel">
      <h3>{mic?.device ?? "Microphone"}</h3>
      {!mic ? (
        <p className="fine">No microphone is connected.</p>
      ) : (
        <>
          <div className="big-number">
            {mic.muted ? "Muted" : mic.level}
            {!mic.muted && <span>%</span>}
          </div>
          <VolumeSlider level={mic.muted ? 0 : mic.level} onChange={set} label="Microphone level" />
          <div className="actions">
            <button onClick={toggle}>{mic.muted ? "Unmute" : "Mute"}</button>
            <button className="link" onClick={() => settingsPage("sound-devices")}>
              Sound devices
            </button>
          </div>
        </>
      )}
    </div>
  );
}

// ---- Light / dark theme ---------------------------------------------------------

function useTheme(preview?: boolean) {
  const [mode, setMode] = usePoll<"light" | "dark">("theme_get", 10_000, "dark", preview);
  const flip = () => {
    if (preview || !mode) return;
    invoke<"light" | "dark">("theme_set", { mode: mode === "light" ? "dark" : "light" }).then(setMode).catch(console.error);
  };
  return { mode, flip };
}

export function ThemeTile({ instance, preview }: WidgetProps) {
  const { mode, flip } = useTheme(preview);
  const next = mode === "light" ? "dark" : "light";
  return (
    <button
      className={`w-action ${instance.size}`}
      title={`Switch Windows to ${next} mode`}
      onClick={(e) => {
        stop(e);
        flip();
      }}
    >
      <span className="glyph">{mode === "light" ? G.sun : G.moon}</span>
      {instance.size === "m" && <span>{mode === "light" ? "Light" : "Dark"}</span>}
    </button>
  );
}

export function ThemePanel() {
  const { mode } = useTheme();
  const set = (m: "light" | "dark") => invoke("theme_set", { mode: m }).catch(console.error);
  return (
    <div className="panel">
      <h3>Windows mode</h3>
      <p className="fine">Switches Windows and your apps between light and dark, like Settings does.</p>
      <div className="seg big">
        <button className={mode === "light" ? "on" : ""} onClick={() => set("light")}>
          <span className="glyph">{G.sun}</span> Light
        </button>
        <button className={mode === "dark" ? "on" : ""} onClick={() => set("dark")}>
          <span className="glyph">{G.moon}</span> Dark
        </button>
      </div>
      <div className="actions">
        <span />
        <button className="link" onClick={() => settingsPage("colors")}>
          Color settings
        </button>
      </div>
    </div>
  );
}

// ---- Show desktop ---------------------------------------------------------------

export function ShowDesktopTile({ instance, preview }: WidgetProps) {
  return (
    <button
      className={`w-action ${instance.size}`}
      title="Show desktop"
      onClick={(e) => {
        stop(e);
        if (!preview) invoke("show_desktop");
      }}
    >
      <span className="glyph">{G.desktop}</span>
      {instance.size === "m" && <span>Desktop</span>}
    </button>
  );
}

export function ShowDesktopPanel() {
  return (
    <div className="panel">
      <h3>Show desktop</h3>
      <p className="fine">Click the tile to minimise every window. Click again to bring them back. Win+D does the same.</p>
    </div>
  );
}

// ---- Day, month and year progress -----------------------------------------------

function progress(now: Date) {
  const day0 = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const day = (now.getTime() - day0.getTime()) / 864e5;
  const dow = (now.getDay() + 6) % 7; // Monday first
  const week = (dow + day) / 7;
  const month0 = new Date(now.getFullYear(), now.getMonth(), 1);
  const month1 = new Date(now.getFullYear(), now.getMonth() + 1, 1);
  const month = (now.getTime() - month0.getTime()) / (month1.getTime() - month0.getTime());
  const year0 = new Date(now.getFullYear(), 0, 1);
  const year1 = new Date(now.getFullYear() + 1, 0, 1);
  const year = (now.getTime() - year0.getTime()) / (year1.getTime() - year0.getTime());
  return { day, week, month, year };
}

export function ProgressTile({ instance }: WidgetProps) {
  const now = new Date(useTick(60_000));
  const p = progress(now);
  if (instance.size === "l") {
    return (
      <div className="w-progress l">
        {(
          [
            ["Day", p.day],
            ["Month", p.month],
            ["Year", p.year],
          ] as const
        ).map(([label, v]) => (
          <span key={label} className="row">
            <small>{label}</small>
            <span className="meter ok thin">
              <i style={{ width: `${Math.max(2, v * 100)}%` }} />
            </span>
            <b>{Math.floor(v * 100)}%</b>
          </span>
        ))}
      </div>
    );
  }
  return (
    <div className={`w-progress ${instance.size}`} title={`${Math.floor(p.day * 100)}% of today has passed`}>
      <Ring percent={p.day * 100} />
      {instance.size === "s" ? (
        <b>{Math.floor(p.day * 100)}%</b>
      ) : (
        <span className="text">
          <b>{Math.floor(p.year * 100)}%</b>
          <small>of {now.getFullYear()}</small>
        </span>
      )}
    </div>
  );
}

export function ProgressPanel() {
  const now = new Date(useTick(30_000));
  const p = progress(now);
  const yearDays = (new Date(now.getFullYear() + 1, 0, 1).getTime() - new Date(now.getFullYear(), 0, 1).getTime()) / 864e5;
  const dayOfYear = Math.floor(p.year * yearDays) + 1;
  const rows: [string, number, string][] = [
    ["Today", p.day, `${24 - now.getHours() - (now.getMinutes() ? 1 : 0)}h ${(60 - now.getMinutes()) % 60}m left`],
    ["This week", p.week, now.toLocaleDateString([], { weekday: "long" })],
    ["This month", p.month, now.toLocaleDateString([], { month: "long", day: "numeric" })],
    [String(now.getFullYear()), p.year, `Day ${dayOfYear} of ${yearDays}`],
  ];
  return (
    <div className="panel w-progress-panel">
      <h3>Progress</h3>
      {rows.map(([label, v, note]) => (
        <div key={label} className="meter-row">
          <div className="meter-label">
            <span>{label}</span>
            <small>{note}</small>
            <b>{(v * 100).toFixed(1)}%</b>
          </div>
          <span className="meter ok">
            <i style={{ width: `${Math.max(1, v * 100)}%` }} />
          </span>
        </div>
      ))}
    </div>
  );
}
