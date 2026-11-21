/**
 * Productivity tools: a countdown timer, a stopwatch with laps, and the day
 * on one line (PC use per hour, now, sunrise and sunset).
 */
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { updateConfig, useTick, useWidgetData, type WidgetProps } from "./shared";
import { PlacePicker, useWeather, type WeatherConfig } from "./weather";

const G = { play: "", pause: "", reset: "", flag: "" };
const stop = (e: React.SyntheticEvent) => e.stopPropagation();

const clock = (ms: number, tenths = false) => {
  const total = Math.max(0, ms);
  const h = Math.floor(total / 3_600_000);
  const m = Math.floor((total % 3_600_000) / 60_000);
  const s = Math.floor((total % 60_000) / 1000);
  const t = Math.floor((total % 1000) / 100);
  const mm = h ? `${h}:${String(m).padStart(2, "0")}` : String(m);
  return `${mm}:${String(s).padStart(2, "0")}${tenths ? `.${t}` : ""}`;
};

// ---- Timer ----------------------------------------------------------------------

type TimerData = { running: boolean; endsAt: number | null; remaining: number; total: number; done: boolean };
type TimerConfig = { minutes?: number };

const timerFresh = (cfg: TimerConfig): TimerData => {
  const total = (cfg.minutes ?? 5) * 60_000;
  return { running: false, endsAt: null, remaining: total, total, done: false };
};
const timerLeft = (d: TimerData, now: number) => (d.running && d.endsAt ? Math.max(0, d.endsAt - now) : d.remaining);

function useTimer(id: string, cfg: TimerConfig, preview?: boolean) {
  const [d, save] = useWidgetData<TimerData>(id, preview ? { ...timerFresh(cfg), remaining: 3 * 60_000 + 20_000 } : timerFresh(cfg), preview);
  const now = useTick(d.running ? 250 : 5000);
  const left = timerLeft(d, now);
  const toggle = () => {
    if (preview) return;
    if (d.running) save({ ...d, running: false, endsAt: null, remaining: left });
    else {
      const base = d.done || left === 0 ? d.total : left;
      save({ ...d, running: true, done: false, endsAt: Date.now() + base, remaining: base });
    }
  };
  const reset = (minutes?: number) => save(timerFresh({ minutes: minutes ?? d.total / 60_000 }));
  return { d, save, left, toggle, reset };
}

export function TimerTile({ instance, preview }: WidgetProps<TimerConfig>) {
  const { d, save, left, toggle } = useTimer(instance.id, instance.config, preview);
  // The dock's tile rings the chime when time is up.
  useEffect(() => {
    if (preview || !d.running || left > 0) return;
    invoke("play_alert");
    save({ ...d, running: false, endsAt: null, remaining: 0, done: true });
  }, [left, d.running, preview]);
  return (
    <div className={`w-timer ${instance.size}${d.running ? " running" : ""}${d.done ? " done" : ""}`}>
      <button
        className="glyph go"
        aria-label={d.running ? "Pause" : "Start"}
        onClick={(e) => {
          stop(e);
          toggle();
        }}
      >
        {d.running ? G.pause : G.play}
      </button>
      <b>{d.done ? "Done" : clock(left)}</b>
      {instance.size === "l" && <span className="meter ok thin"><i style={{ width: `${(1 - left / d.total) * 100}%` }} /></span>}
    </div>
  );
}

export function TimerPanel({ instance }: WidgetProps<TimerConfig>) {
  const { d, left, toggle, reset } = useTimer(instance.id, instance.config);
  const presets = [1, 3, 5, 10, 15, 25, 45, 60];
  const [custom, setCustom] = useState("");
  const set = (m: number) => {
    updateConfig(instance.id, { ...instance.config, minutes: m });
    reset(m);
  };
  return (
    <div className="panel w-timer-panel">
      <h3>Timer</h3>
      <div className="big-time">{d.done ? "Time's up" : clock(left)}</div>
      <span className="meter ok">
        <i style={{ width: `${(1 - left / d.total) * 100}%` }} />
      </span>
      <div className="row-buttons">
        <button className="primary" onClick={toggle}>
          {d.running ? "Pause" : d.done || left === d.total ? "Start" : "Resume"}
        </button>
        <button onClick={() => reset()}>Reset</button>
      </div>
      <div className="presets">
        {presets.map((m) => (
          <button key={m} className={d.total === m * 60_000 ? "on" : ""} onClick={() => set(m)}>
            {m < 60 ? `${m}m` : "1h"}
          </button>
        ))}
      </div>
      <form
        className="svc-fields"
        onSubmit={(e) => {
          e.preventDefault();
          const m = Number(custom);
          if (m > 0 && m <= 24 * 60) set(m);
          setCustom("");
        }}
      >
        <label>
          <span>Minutes</span>
          <input inputMode="decimal" placeholder="e.g. 7" value={custom} onChange={(e) => setCustom(e.target.value)} />
        </label>
      </form>
      <p className="fine">A chime plays when time is up, even with this closed.</p>
    </div>
  );
}

// ---- Stopwatch ------------------------------------------------------------------------

type WatchData = { startedAt: number | null; before: number; laps: number[] };
const watchFresh: WatchData = { startedAt: null, before: 0, laps: [] };
const elapsed = (d: WatchData, now: number) => d.before + (d.startedAt ? now - d.startedAt : 0);

function useWatch(id: string, preview?: boolean) {
  const [d, save] = useWidgetData<WatchData>(id, preview ? { startedAt: null, before: 83_400, laps: [41_200] } : watchFresh, preview);
  const now = useTick(d.startedAt ? 100 : 5000);
  const t = elapsed(d, now);
  const toggle = () => {
    if (preview) return;
    save(d.startedAt ? { ...d, startedAt: null, before: t } : { ...d, startedAt: Date.now() });
  };
  const lap = () => save({ ...d, laps: [...d.laps, t] });
  const reset = () => save(watchFresh);
  return { d, t, toggle, lap, reset };
}

export function StopwatchTile({ instance, preview }: WidgetProps) {
  const { d, t, toggle } = useWatch(instance.id, preview);
  return (
    <div className={`w-timer ${instance.size}${d.startedAt ? " running" : ""}`}>
      <button
        className="glyph go"
        aria-label={d.startedAt ? "Pause" : "Start"}
        onClick={(e) => {
          stop(e);
          toggle();
        }}
      >
        {d.startedAt ? G.pause : G.play}
      </button>
      <b>{clock(t, instance.size !== "s")}</b>
      {instance.size === "l" && d.laps.length > 0 && <small>Lap {d.laps.length}</small>}
    </div>
  );
}

export function StopwatchPanel({ instance }: WidgetProps) {
  const { d, t, toggle, lap, reset } = useWatch(instance.id);
  const splits = d.laps.map((v, i) => v - (i ? d.laps[i - 1] : 0));
  const best = splits.length > 1 ? Math.min(...splits) : -1;
  return (
    <div className="panel w-timer-panel">
      <h3>Stopwatch</h3>
      <div className="big-time">{clock(t, true)}</div>
      <div className="row-buttons">
        <button className="primary" onClick={toggle}>
          {d.startedAt ? "Pause" : t ? "Resume" : "Start"}
        </button>
        <button disabled={!d.startedAt} onClick={lap}>
          Lap
        </button>
        <button disabled={!t} onClick={reset}>
          Reset
        </button>
      </div>
      <ul className="laps">
        {splits
          .map((s, i) => (
            <li key={i} className={s === best ? "best" : ""}>
              <span>Lap {i + 1}</span>
              <span>{clock(s, true)}</span>
              <b>{clock(d.laps[i], true)}</b>
            </li>
          ))
          .reverse()}
      </ul>
    </div>
  );
}

// ---- Day timeline -----------------------------------------------------------------------

type Today = { total: number; hours: number[] };

function useToday(preview?: boolean) {
  const [t, setT] = useState<Today | null>(
    preview ? { total: 6 * 3600, hours: [0, 0, 0, 0, 0, 0, 0, 0, 900, 2400, 3000, 2600, 1200, 2200, 3100, 2000, 0, 0, 0, 0, 0, 0, 0, 0] } : null,
  );
  useEffect(() => {
    if (preview) return;
    const load = () => invoke<Today>("screen_time_today").then(setT).catch(() => {});
    load();
    const id = setInterval(load, 60_000);
    return () => clearInterval(id);
  }, [preview]);
  return t;
}

const hourOf = (t?: string) => (t ? Number(t.slice(11, 13)) + Number(t.slice(14, 16)) / 60 : null);

function Timeline({ hours, now, rise, set, tall }: { hours: number[]; now: Date; rise: number | null; set: number | null; tall?: boolean }) {
  const h = now.getHours() + now.getMinutes() / 60;
  return (
    <div className={`timeline${tall ? " tall" : ""}`}>
      {rise !== null && set !== null && <span className="daylight" style={{ left: `${(rise / 24) * 100}%`, width: `${((set - rise) / 24) * 100}%` }} />}
      {/* 6 am, noon and 6 pm */}
      {[6, 12, 18].map((x) => (
        <span key={x} className="tick" style={{ left: `${(x / 24) * 100}%` }} />
      ))}
      {hours.map((s, i) => (
        <i
          key={i}
          style={{ height: `${Math.max(s > 0 ? 14 : 0, Math.min(1, s / 3600) * 100)}%` }}
          className={i > h ? "future" : s >= 2700 ? "busy" : ""}
        />
      ))}
      <span className="nowline" style={{ left: `${(h / 24) * 100}%` }}>
        <span className="now-dot" />
      </span>
    </div>
  );
}

const hmShort = (s: number) => (s >= 3600 ? `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m` : `${Math.floor(s / 60)}m`);

export function DayTimelineTile({ instance, preview }: WidgetProps<WeatherConfig>) {
  const now = new Date(useTick(60_000));
  const t = useToday(preview);
  const { w } = useWeather(instance.config, preview);
  return (
    <div className={`w-timeline ${instance.size}`} title={t ? `On the PC ${hmShort(t.total)} today` : "Your day"}>
      {instance.size === "l" && (
        <span className="tl-total">
          <b>{t ? hmShort(t.total) : "–"}</b>
          <small>today</small>
        </span>
      )}
      <Timeline hours={t?.hours ?? Array(24).fill(0)} now={now} rise={hourOf(w?.days[0]?.sunrise)} set={hourOf(w?.days[0]?.sunset)} />
    </div>
  );
}

export function DayTimelinePanel({ instance }: WidgetProps<WeatherConfig>) {
  const now = new Date(useTick(60_000));
  const t = useToday();
  const { w } = useWeather(instance.config);
  const [editing, setEditing] = useState(false);
  const hm = (s: number) => `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`;
  const rise = w?.days[0]?.sunrise;
  const set = w?.days[0]?.sunset;
  const fmt = (x: string) => new Date(2000, 0, 1, Number(x.slice(11, 13)), Number(x.slice(14, 16))).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  return (
    <div className="panel w-timeline-panel">
      <h3>
        Your day <span className="now">{now.toLocaleDateString([], { weekday: "long", month: "short", day: "numeric" })}</span>
      </h3>
      <Timeline hours={t?.hours ?? Array(24).fill(0)} now={now} rise={hourOf(rise)} set={hourOf(set)} tall />
      <div className="axis24">
        {[0, 6, 12, 18, 24].map((x) => (
          <span key={x}>{new Date(2000, 0, 1, x % 24).toLocaleTimeString([], { hour: "numeric" })}</span>
        ))}
      </div>
      <div className="reading">
        <div className="line">
          <span>On the PC today</span>
          <b>{t ? hm(t.total) : "–"}</b>
        </div>
        <div className="line">
          <span>Day left</span>
          <b>{hm((24 - now.getHours()) * 3600 - now.getMinutes() * 60)}</b>
        </div>
        {rise && set && (
          <div className="line">
            <span>Sunrise · sunset</span>
            <b>
              {fmt(rise)} · {fmt(set)}
            </b>
          </div>
        )}
      </div>
      {editing || !instance.config.place ? (
        <>
          <p className="fine">Choose a city to shade daylight on the line (optional).</p>
          <PlacePicker id={instance.id} cfg={instance.config} />
        </>
      ) : (
        <button className="link" onClick={() => setEditing(true)}>
          Change city ({instance.config.place.name})
        </button>
      )}
      <p className="fine">Bars are your PC use per hour, from Screen time. Kept on this PC.</p>
    </div>
  );
}
