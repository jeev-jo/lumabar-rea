import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSystemStatus, type Status } from "../system";
import { useTick, type WidgetProps } from "./shared";

/**
 * Widgets for the PC itself: battery, speakers, quick toggles, now playing,
 * screen time and wallpaper. Live values come from the system status stream.
 */

const G = {
  bolt: "",
  wifi: "",
  bluetooth: "",
  chevron: "",
  mute: "",
  vol: ["", "", "", ""],
  play: "",
  pause: "",
  prev: "",
  next: "",
  music: "",
  watch: "",
  photo: "",
};

const settingsPage = (page: string) => invoke("open_ms_settings", { page }).catch(console.error);
/** Controls inside a tile must not also open the tile's details. */
const stop = (e: React.SyntheticEvent) => e.stopPropagation();

const DEMO: Status = {
  volume: { level: 8, muted: false, device: "Speakers" },
  network: "wifi",
  battery: { percent: 97, charging: true, minutes_left: null },
  media: { title: "Night Drive", artist: "Neon Coast", playing: true, art: null },
  wifi: true,
  bluetooth: false,
};

function useStatus(preview?: boolean) {
  const live = useSystemStatus();
  return preview ? DEMO : live;
}

const hm = (mins: number) => (mins >= 60 ? `${Math.floor(mins / 60)}h ${mins % 60}m` : `${mins}m`);

// ---- Battery ---------------------------------------------------------------

export function BatteryTile({ instance, preview }: WidgetProps) {
  const b = useStatus(preview)?.battery;
  if (!b) return <div className="w-battery none">No battery</div>;
  const low = b.percent <= 15 && !b.charging;
  return (
    <div className={`w-battery ${instance.size}${low ? " low" : ""}`} title={`Battery ${b.percent}%${b.charging ? ", charging" : ""}`}>
      {b.charging && <span className="glyph">{G.bolt}</span>}
      <b>{b.percent}</b>
      {instance.size !== "s" && (
        <small>{b.charging ? "Charging" : b.minutes_left != null ? `${hm(b.minutes_left)} left` : "On battery"}</small>
      )}
    </div>
  );
}

export function BatteryPanel() {
  const b = useSystemStatus()?.battery;
  return (
    <div className="panel">
      <h3>Battery</h3>
      {!b ? (
        <p className="fine">This PC has no battery.</p>
      ) : (
        <>
          <div className="big-number">
            {b.percent}
            <span>%</span>
          </div>
          <span className={`meter ${b.percent <= 15 && !b.charging ? "critical" : b.percent <= 30 ? "warning" : "ok"}`}>
            <i style={{ width: `${b.percent}%` }} />
          </span>
          <p className="fine">
            {b.charging ? "Plugged in and charging." : b.minutes_left != null ? `About ${hm(b.minutes_left)} left.` : "Running on battery."}
          </p>
        </>
      )}
      <div className="actions">
        <span />
        <button className="link" onClick={() => settingsPage("batterysaver")}>
          Battery settings
        </button>
      </div>
    </div>
  );
}

// ---- Speakers ----------------------------------------------------------------

function volumeGlyph(level: number, muted: boolean) {
  if (muted || level === 0) return G.mute;
  return G.vol[level < 34 ? 1 : level < 67 ? 2 : 3];
}

function VolumeSlider({ level, onChange }: { level: number; onChange: (v: number) => void }) {
  return (
    <input
      className="slider"
      type="range"
      min={0}
      max={100}
      value={level}
      style={{ ["--fill" as string]: `${level}%` }}
      onChange={(e) => onChange(Number(e.target.value))}
      onClick={stop}
      onPointerDown={stop}
      onMouseDown={stop}
      aria-label="Volume"
    />
  );
}

/** Volume with an optimistic local value, so the slider doesn't jump back while dragging. */
function useVolume(preview?: boolean) {
  const v = useStatus(preview)?.volume ?? null;
  const [local, setLocal] = useState<number | null>(null);
  useEffect(() => {
    const t = setTimeout(() => setLocal(null), 1200);
    return () => clearTimeout(t);
  }, [local]);
  const set = (level: number) => {
    setLocal(level);
    if (!preview) invoke("volume_set", { level }).catch(console.error);
  };
  return { v, level: local ?? v?.level ?? 0, set };
}

export function SpeakersTile({ instance, preview }: WidgetProps) {
  const { v, level, set } = useVolume(preview);
  if (!v) return <div className="w-speakers">No sound output</div>;
  return (
    <div className={`w-speakers ${instance.size}`}>
      <div className="top">
        <button
          className="glyph mute"
          title={v.muted ? "Unmute" : "Mute"}
          onClick={(e) => {
            stop(e);
            if (!preview) invoke("volume_toggle_mute");
          }}
        >
          {volumeGlyph(level, v.muted)}
        </button>
        <b>{v.device}</b>
        <small>{v.muted ? "Muted" : `${level}%`}</small>
      </div>
      <VolumeSlider level={v.muted ? 0 : level} onChange={set} />
    </div>
  );
}

export function SpeakersPanel() {
  const { v, level, set } = useVolume();
  return (
    <div className="panel">
      <h3>{v?.device ?? "Sound"}</h3>
      {v && (
        <>
          <div className="big-number">
            {v.muted ? "Muted" : level}
            {!v.muted && <span>%</span>}
          </div>
          <VolumeSlider level={v.muted ? 0 : level} onChange={set} />
          <div className="actions">
            <button onClick={() => invoke("volume_toggle_mute")}>{v.muted ? "Unmute" : "Mute"}</button>
            <button className="link" onClick={() => settingsPage("sound")}>
              Sound settings
            </button>
          </div>
        </>
      )}
    </div>
  );
}

// ---- Quick toggles -----------------------------------------------------------

function RadioPill({ kind, on, preview }: { kind: "wifi" | "bluetooth"; on: boolean | null | undefined; preview?: boolean }) {
  const [error, setError] = useState<string | null>(null);
  if (on == null) return null;
  const label = kind === "wifi" ? "Wi-Fi" : "Bluetooth";
  return (
    <span className={`pill-toggle${on ? " on" : ""}`} title={error ?? `${label} ${on ? "on" : "off"}`}>
      <button
        className="glyph main"
        aria-pressed={on}
        aria-label={`${label} ${on ? "on" : "off"}`}
        onClick={(e) => {
          stop(e);
          if (preview) return;
          invoke("radio_set", { kind, on: !on })
            .then(() => setError(null))
            .catch((err) => setError(String(err)));
        }}
      >
        {kind === "wifi" ? G.wifi : G.bluetooth}
      </button>
      <button
        className="glyph more"
        aria-label={`${label} settings`}
        onClick={(e) => {
          stop(e);
          if (!preview) settingsPage(kind === "wifi" ? "network-wifi" : "bluetooth");
        }}
      >
        {G.chevron}
      </button>
    </span>
  );
}

export function TogglesTile({ preview }: WidgetProps) {
  const s = useStatus(preview);
  if (!s || (s.wifi == null && s.bluetooth == null)) return <div className="w-toggles">No radios</div>;
  return (
    <div className="w-toggles">
      <RadioPill kind="wifi" on={s.wifi} preview={preview} />
      <RadioPill kind="bluetooth" on={s.bluetooth} preview={preview} />
    </div>
  );
}

export function TogglesPanel() {
  const s = useSystemStatus();
  const row = (kind: "wifi" | "bluetooth", label: string, on: boolean | null | undefined) =>
    on == null ? null : (
      <div className="toggle-row">
        <span className="glyph">{kind === "wifi" ? G.wifi : G.bluetooth}</span>
        <span className="name">
          <b>{label}</b>
          <small>{on ? "On" : "Off"}</small>
        </span>
        <RadioPill kind={kind} on={on} />
      </div>
    );
  return (
    <div className="panel">
      <h3>Quick toggles</h3>
      {row("wifi", "Wi-Fi", s?.wifi)}
      {row("bluetooth", "Bluetooth", s?.bluetooth)}
      <p className="fine">Click to switch on or off; the arrow opens its Windows settings.</p>
    </div>
  );
}

// ---- Now playing ---------------------------------------------------------------

function Art({ src, size }: { src: string | null | undefined; size: number }) {
  return src ? (
    <img className="art" src={src} alt="" width={size} height={size} draggable={false} />
  ) : (
    <span className="art empty glyph" style={{ width: size, height: size }}>
      {G.music}
    </span>
  );
}

const control = (action: string) => invoke("media_control", { action }).catch(console.error);

export function NowPlayingTile({ instance, preview }: WidgetProps) {
  const m = useStatus(preview)?.media;
  if (!m) {
    return (
      <div className={`w-now ${instance.size} idle`}>
        <Art src={null} size={30} />
        {instance.size !== "s" && <small>Nothing playing</small>}
      </div>
    );
  }
  return (
    <div className={`w-now ${instance.size}${m.playing ? " playing" : ""}`} title={m.artist ? `${m.title} — ${m.artist}` : m.title}>
      <span className="art-wrap">
        <Art src={m.art} size={30} />
        <button
          className="glyph overlay"
          aria-label={m.playing ? "Pause" : "Play"}
          onClick={(e) => {
            stop(e);
            if (!preview) control("toggle");
          }}
        >
          {m.playing ? G.pause : G.play}
        </button>
      </span>
      {instance.size !== "s" && (
        <span className="text">
          <b>{m.title}</b>
          {m.artist && <small>{m.artist}</small>}
        </span>
      )}
      {instance.size === "l" && (
        <button
          className="glyph skip"
          aria-label="Next"
          onClick={(e) => {
            stop(e);
            if (!preview) control("next");
          }}
        >
          {G.next}
        </button>
      )}
    </div>
  );
}

export function NowPlayingPanel() {
  const m = useSystemStatus()?.media;
  if (!m) {
    return (
      <div className="panel">
        <h3>Now playing</h3>
        <p className="fine">Nothing is playing. Start music or a video in any app that shows media controls.</p>
      </div>
    );
  }
  return (
    <div className="panel now-panel">
      <Art src={m.art} size={140} />
      <b className="title">{m.title}</b>
      {m.artist && <span className="artist">{m.artist}</span>}
      <div className="transport">
        <button className="glyph" aria-label="Previous" onClick={() => control("previous")}>
          {G.prev}
        </button>
        <button className="glyph primary" aria-label={m.playing ? "Pause" : "Play"} onClick={() => control("toggle")}>
          {m.playing ? G.pause : G.play}
        </button>
        <button className="glyph" aria-label="Next" onClick={() => control("next")}>
          {G.next}
        </button>
      </div>
    </div>
  );
}

// ---- Screen time ---------------------------------------------------------------

type Today = { total: number; hours: number[]; apps: { name: string; exe: string; secs: number }[] };

const DEMO_TODAY: Today = {
  total: 8400,
  hours: [0, 0, 0, 0, 0, 0, 0, 0, 0, 1200, 2400, 1800, 600, 900, 1500, 0, 0, 0, 0, 0, 0, 0, 0, 0],
  apps: [],
};

function useToday(preview?: boolean) {
  const [t, setT] = useState<Today | null>(preview ? DEMO_TODAY : null);
  useEffect(() => {
    if (preview) return;
    const load = () => invoke<Today>("screen_time_today").then(setT).catch(console.error);
    load();
    const id = setInterval(load, 60_000);
    return () => clearInterval(id);
  }, [preview]);
  return t;
}

const dur = (secs: number) => {
  const m = Math.round(secs / 60);
  return m >= 60 ? `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, "0")}m` : `${m}m`;
};

/** Hourly bars: one series, rounded ends on the baseline, a 2px gap between bars. */
function HourBars({ hours, from = 0, to = 24, height = 22, hover }: { hours: number[]; from?: number; to?: number; height?: number; hover?: boolean }) {
  const [tip, setTip] = useState<number | null>(null);
  const slice = hours.slice(from, to);
  const max = Math.max(60, ...slice);
  const n = slice.length;
  const W = n * 6;
  return (
    <span className="hour-bars">
      <svg viewBox={`0 0 ${W} ${height}`} width="100%" height={height} preserveAspectRatio="none" onMouseLeave={() => setTip(null)}>
        {slice.map((v, i) => {
          const h = v > 0 ? Math.max(2, (v / max) * (height - 1)) : 0;
          return (
            <g key={i} onMouseEnter={() => hover && setTip(i)}>
              <rect x={i * 6} y={0} width={6} height={height} fill="transparent" />
              {h > 0 && <rect x={i * 6 + 1} y={height - h} width={4} height={h} rx={1.5} className={tip === i ? "hb-bar on" : "hb-bar"} />}
            </g>
          );
        })}
      </svg>
      {hover && tip !== null && (
        <span className="bar-tip" style={{ left: `${((tip + 0.5) / n) * 100}%` }}>
          {String(from + tip).padStart(2, "0")}:00 · {dur(slice[tip])}
        </span>
      )}
    </span>
  );
}

export function ScreenTimeTile({ instance, preview }: WidgetProps) {
  const t = useToday(preview);
  const hour = new Date(useTick(60_000)).getHours();
  return (
    <div className={`w-screentime ${instance.size}`}>
      <span className="glyph watch">{G.watch}</span>
      <span className="text">
        {instance.size !== "s" && <small>Screen time</small>}
        <b>{t ? dur(t.total) : "…"}</b>
      </span>
      {instance.size !== "s" && t && <HourBars hours={t.hours} from={Math.max(0, hour - 9)} to={hour + 1} />}
    </div>
  );
}

function AppIcon({ exe }: { exe: string }) {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    invoke<string | null>("app_icon", { path: exe }).then(setSrc).catch(() => {});
  }, [exe]);
  return src ? <img src={src} alt="" width={18} height={18} /> : <span className="glyph">{G.watch}</span>;
}

export function ScreenTimePanel() {
  const t = useToday();
  if (!t) return null;
  const top = t.apps[0]?.secs ?? 1;
  return (
    <div className="panel">
      <h3>
        Screen time <span className="now">today</span>
      </h3>
      <div className="big-number">{dur(t.total)}</div>
      <HourBars hours={t.hours} height={56} hover />
      <div className="hour-axis">
        <span>00</span>
        <span>06</span>
        <span>12</span>
        <span>18</span>
        <span>24</span>
      </div>
      <ul className="app-usage">
        {t.apps.map((a) => (
          <li key={a.exe}>
            <AppIcon exe={a.exe} />
            <span className="name">{a.name}</span>
            <span className="meter">
              <i style={{ width: `${(a.secs / top) * 100}%` }} />
            </span>
            <b>{dur(a.secs)}</b>
          </li>
        ))}
      </ul>
      <p className="fine">Counted while you use the PC; kept only on this PC.</p>
    </div>
  );
}

// ---- Wallpaper -------------------------------------------------------------------

function useWallpaper(preview?: boolean) {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    if (preview) return;
    const load = () => invoke<string | null>("wallpaper_image").then(setSrc).catch(() => {});
    load();
    const id = setInterval(load, 5 * 60_000);
    return () => clearInterval(id);
  }, [preview]);
  return src;
}

export function WallpaperTile({ instance, preview }: WidgetProps) {
  const src = useWallpaper(preview);
  return (
    <div className={`w-wallpaper ${instance.size}`}>
      {src ? <img src={src} alt="" draggable={false} /> : <span className="glyph">{G.photo}</span>}
    </div>
  );
}

export function WallpaperPanel() {
  const src = useWallpaper();
  return (
    <div className="panel">
      <h3>Wallpaper</h3>
      {src ? <img className="wallpaper-big" src={src} alt="Current wallpaper" /> : <p className="fine">No wallpaper picture is set.</p>}
      <div className="actions">
        <span />
        <button className="link" onClick={() => settingsPage("personalization-background")}>
          Change wallpaper
        </button>
      </div>
    </div>
  );
}
