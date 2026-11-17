import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { updateConfig, useTick, useWidgetData, type WidgetProps } from "./shared";

type Phase = "focus" | "break";
type Data = { phase: Phase; running: boolean; endsAt: number | null; remaining: number; rounds: number };
export type PomodoroConfig = { focus?: number; break?: number };

const minutes = (cfg: PomodoroConfig, phase: Phase) => (phase === "focus" ? (cfg.focus ?? 25) : (cfg.break ?? 5));
const fresh = (cfg: PomodoroConfig): Data => ({
  phase: "focus",
  running: false,
  endsAt: null,
  remaining: minutes(cfg, "focus") * 60_000,
  rounds: 0,
});

function left(d: Data, now: number) {
  return d.running && d.endsAt ? Math.max(0, d.endsAt - now) : d.remaining;
}

const mmss = (ms: number) => {
  const s = Math.ceil(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};

function Ring({ fraction, phase }: { fraction: number; phase: Phase }) {
  const r = 9;
  const c = 2 * Math.PI * r;
  return (
    <svg className={`ring ${phase}`} viewBox="0 0 24 24" width="22" height="22" aria-hidden>
      <circle cx="12" cy="12" r={r} className="track" />
      <circle cx="12" cy="12" r={r} className="fill" strokeDasharray={c} strokeDashoffset={c * (1 - fraction)} transform="rotate(-90 12 12)" />
    </svg>
  );
}

export function PomodoroTile({ instance, preview }: WidgetProps<PomodoroConfig>) {
  const cfg = instance.config;
  const [data, save] = useWidgetData<Data>(instance.id, preview ? { ...fresh(cfg), remaining: 17 * 60_000 + 42_000 } : fresh(cfg), preview);
  const now = useTick(500);
  const ms = left(data, now);

  // The dock's tile is the one place that ends a phase, so the alert plays once.
  useEffect(() => {
    if (preview || !data.running || ms > 0) return;
    const next: Phase = data.phase === "focus" ? "break" : "focus";
    invoke("play_alert");
    save({
      phase: next,
      running: true,
      endsAt: Date.now() + minutes(cfg, next) * 60_000,
      remaining: minutes(cfg, next) * 60_000,
      rounds: data.phase === "focus" ? data.rounds + 1 : data.rounds,
    });
  }, [ms <= 0, data.running]);

  const total = minutes(cfg, data.phase) * 60_000;
  return (
    <div className={`w-pomodoro ${instance.size}${data.running ? " running" : ""}`}>
      {instance.size !== "s" && <Ring fraction={1 - ms / total} phase={data.phase} />}
      <b>{mmss(ms)}</b>
      {instance.size === "l" && <span className="phase">{data.phase === "focus" ? "Focus" : "Break"}</span>}
    </div>
  );
}

export function PomodoroPanel({ instance }: WidgetProps<PomodoroConfig>) {
  const cfg = instance.config;
  const [data, save] = useWidgetData<Data>(instance.id, fresh(cfg));
  const now = useTick(500);
  const ms = left(data, now);
  const start = () => save({ ...data, running: true, endsAt: Date.now() + data.remaining });
  const pause = () => save({ ...data, running: false, endsAt: null, remaining: ms });
  const reset = () => save(fresh(cfg));
  const setLen = (key: "focus" | "break", v: number) => updateConfig(instance.id, { ...cfg, [key]: v });

  return (
    <div className="panel w-pomodoro-panel">
      <h3>
        {data.phase === "focus" ? "Focus" : "Break"}
        <span className="now">{data.rounds} rounds today</span>
      </h3>
      <div className="big">{mmss(ms)}</div>
      <div className="buttons">
        {data.running ? <button onClick={pause}>Pause</button> : <button className="primary" onClick={start}>Start</button>}
        <button onClick={reset}>Reset</button>
      </div>
      <div className="lengths">
        <label>
          Focus
          <select value={cfg.focus ?? 25} onChange={(e) => setLen("focus", Number(e.target.value))}>
            {[15, 20, 25, 30, 45, 50, 60].map((m) => (
              <option key={m} value={m}>
                {m} min
              </option>
            ))}
          </select>
        </label>
        <label>
          Break
          <select value={cfg.break ?? 5} onChange={(e) => setLen("break", Number(e.target.value))}>
            {[3, 5, 10, 15].map((m) => (
              <option key={m} value={m}>
                {m} min
              </option>
            ))}
          </select>
        </label>
      </div>
    </div>
  );
}
