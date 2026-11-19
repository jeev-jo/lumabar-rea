/**
 * Clocks: digital, flip, seven-segment and analog clocks, the date as text or
 * as a calendar page, one city in large digits, analog world clocks, and tiles
 * that pair the time or the week with the weather.
 */
import { useEffect, useRef, useState } from "react";
import { updateConfig, useTick, type WidgetProps } from "./shared";
import { deg, describe, PlacePicker, useWeather, type WeatherConfig } from "./weather";
import { WorldClockPanel } from "./worldclock";

type ClockConfig = { h24?: boolean; seconds?: boolean };
type Zone = { label: string; tz: string };
type ZonesConfig = { zones?: Zone[] };

const DEFAULT_ZONES: Zone[] = [
  { label: "London", tz: "Europe/London" },
  { label: "New York", tz: "America/New_York" },
  { label: "Tokyo", tz: "Asia/Tokyo" },
];
const zonesOf = (cfg: ZonesConfig) => (cfg.zones?.length ? cfg.zones : DEFAULT_ZONES);

/** Hours, minutes, seconds of `now` in a time zone (or here). */
function parts(now: number, tz?: string) {
  const f = new Intl.DateTimeFormat("en-GB", { hour: "2-digit", minute: "2-digit", second: "2-digit", hourCycle: "h23", timeZone: tz });
  const p = Object.fromEntries(f.formatToParts(new Date(now)).map((x) => [x.type, x.value]));
  return { h: Number(p.hour), m: Number(p.minute), s: Number(p.second) };
}

function hhmm(now: number, h24: boolean, seconds = false, tz?: string) {
  const { h, m, s } = parts(now, tz);
  const hh = h24 ? String(h).padStart(2, "0") : String(h % 12 || 12);
  return `${hh}:${String(m).padStart(2, "0")}${seconds ? `:${String(s).padStart(2, "0")}` : ""}`;
}

const ampm = (now: number, tz?: string) => (parts(now, tz).h < 12 ? "AM" : "PM");

/** Whether this PC's region writes times in 24-hour form. */
const local24 = () => !new Intl.DateTimeFormat([], { hour: "numeric" }).resolvedOptions().hour12;
const is24 = (cfg: ClockConfig) => cfg.h24 ?? local24();

function utcOffset(tz: string, now: number) {
  const name = new Intl.DateTimeFormat("en-US", { timeZone: tz, timeZoneName: "shortOffset" })
    .formatToParts(new Date(now))
    .find((p) => p.type === "timeZoneName")?.value;
  return name?.replace("GMT", "UTC") ?? "";
}

// ---- Clock --------------------------------------------------------------------

export function ClockTile({ instance }: WidgetProps<ClockConfig>) {
  const now = useTick(1000);
  const h24 = is24(instance.config);
  const secs = instance.config.seconds ?? instance.size !== "s";
  return (
    <div className={`w-clock ${instance.size}`}>
      <b className="time">
        {hhmm(now, h24, secs && instance.size !== "s")}
        {!h24 && instance.size !== "s" && <small> {ampm(now)}</small>}
      </b>
      {instance.size === "l" && <small className="date">{new Date(now).toLocaleDateString([], { weekday: "short", month: "short", day: "numeric" })}</small>}
    </div>
  );
}

function MonthGrid({ now }: { now: number }) {
  const d = new Date(now);
  const first = new Date(d.getFullYear(), d.getMonth(), 1);
  const lead = (first.getDay() + 6) % 7; // Monday first
  const days = new Date(d.getFullYear(), d.getMonth() + 1, 0).getDate();
  const names = Array.from({ length: 7 }, (_, i) => new Date(2024, 0, 1 + i).toLocaleDateString([], { weekday: "narrow" }));
  return (
    <div className="month">
      {names.map((n, i) => (
        <span key={`n${i}`} className="dow">
          {n}
        </span>
      ))}
      {Array.from({ length: lead }, (_, i) => (
        <span key={`b${i}`} />
      ))}
      {Array.from({ length: days }, (_, i) => (
        <span key={i} className={i + 1 === d.getDate() ? "today" : ""}>
          {i + 1}
        </span>
      ))}
    </div>
  );
}

export function ClockPanel({ instance }: WidgetProps<ClockConfig>) {
  const now = useTick(1000);
  const h24 = is24(instance.config);
  const set = (patch: ClockConfig) => updateConfig(instance.id, { ...instance.config, ...patch });
  return (
    <div className="panel w-clock-panel">
      <div className="big-time">
        {hhmm(now, h24, true)}
        {!h24 && <small>{ampm(now)}</small>}
      </div>
      <div className="big-date">{new Date(now).toLocaleDateString([], { weekday: "long", year: "numeric", month: "long", day: "numeric" })}</div>
      <MonthGrid now={now} />
      <div className="seg" role="radiogroup" aria-label="Clock format">
        <button className={!h24 ? "on" : ""} onClick={() => set({ h24: false })}>
          12-hour
        </button>
        <button className={h24 ? "on" : ""} onClick={() => set({ h24: true })}>
          24-hour
        </button>
      </div>
    </div>
  );
}

// ---- Flip clock ------------------------------------------------------------------

function FlipDigit({ value }: { value: string }) {
  const [shown, setShown] = useState(value);
  const [flipping, setFlipping] = useState<string | null>(null);
  const prev = useRef(value);
  useEffect(() => {
    if (value === prev.current) return;
    setFlipping(prev.current);
    prev.current = value;
    const t = setTimeout(() => {
      setShown(value);
      setFlipping(null);
    }, 520);
    return () => clearTimeout(t);
  }, [value]);
  return (
    <span className={`flip${flipping !== null ? " go" : ""}`}>
      <span className="top">{value}</span>
      <span className="bottom">{flipping ?? shown}</span>
      {flipping !== null && (
        <>
          <span className="leaf-top">{flipping}</span>
          <span className="leaf-bottom">{value}</span>
        </>
      )}
    </span>
  );
}

export function FlipClockTile({ instance }: WidgetProps<ClockConfig>) {
  const now = useTick(1000);
  const t = hhmm(now, is24(instance.config)).padStart(5, "0");
  const digits = instance.size === "s" ? [t.slice(0, 2), t.slice(3, 5)] : [t[0], t[1], ":", t[3], t[4]];
  return (
    <div className={`w-flip ${instance.size}`}>
      {digits.map((d, i) => (d === ":" ? <i key={i} className="colon" /> : <FlipDigit key={i} value={d} />))}
    </div>
  );
}

// ---- Seven-segment clock --------------------------------------------------------------

const SEGMENTS: Record<string, string> = {
  "0": "abcdef",
  "1": "bc",
  "2": "abged",
  "3": "abgcd",
  "4": "fgbc",
  "5": "afgcd",
  "6": "afgedc",
  "7": "abc",
  "8": "abcdefg",
  "9": "abcdfg",
};

function SegDigit({ d }: { d: string }) {
  const on = SEGMENTS[d] ?? "";
  const seg = (k: string, x: number, y: number, w: number, h: number) => (
    <rect key={k} x={x} y={y} width={w} height={h} rx={1} className={on.includes(k) ? "on" : "off"} />
  );
  return (
    <svg className="segdigit" viewBox="0 0 12 22" aria-hidden>
      {seg("a", 2, 0, 8, 2)}
      {seg("b", 10, 2, 2, 8)}
      {seg("c", 10, 12, 2, 8)}
      {seg("d", 2, 20, 8, 2)}
      {seg("e", 0, 12, 2, 8)}
      {seg("f", 0, 2, 2, 8)}
      {seg("g", 2, 10, 8, 2)}
    </svg>
  );
}

export function SegmentClockTile({ instance }: WidgetProps<ClockConfig>) {
  const now = useTick(1000);
  const t = hhmm(now, is24(instance.config)).padStart(5, "0");
  const d = new Date(now);
  return (
    <div className={`w-seg ${instance.size}`}>
      <span className="digits">
        <SegDigit d={t[0]} />
        <SegDigit d={t[1]} />
        <i className={`colon${d.getSeconds() % 2 ? " dim" : ""}`} />
        <SegDigit d={t[3]} />
        <SegDigit d={t[4]} />
      </span>
      {instance.size !== "s" && (
        <span className="side">
          <b>{d.toLocaleDateString([], { weekday: "short" }).toUpperCase()}</b>
          <small>{d.toLocaleDateString([], { month: "short", day: "numeric" })}</small>
        </span>
      )}
    </div>
  );
}

// ---- Analog clocks --------------------------------------------------------------------

function Face({ now, tz, size, minimal, seconds }: { now: number; tz?: string; size: number; minimal?: boolean; seconds?: boolean }) {
  const { h, m, s } = parts(now, tz);
  const c = size / 2;
  const hand = (angle: number, len: number, cls: string) => {
    const a = ((angle - 90) * Math.PI) / 180;
    return <line className={cls} x1={c} y1={c} x2={c + Math.cos(a) * len} y2={c + Math.sin(a) * len} />;
  };
  const night = h < 6 || h >= 19;
  return (
    <svg className={`face${minimal ? " minimal" : ""}${night ? " night" : ""}`} width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden>
      <circle cx={c} cy={c} r={c - 0.75} className="dial" />
      {Array.from({ length: 12 }, (_, i) => {
        const a = (i * 30 * Math.PI) / 180;
        const long = i % 3 === 0;
        const r1 = c - (long ? 4.5 : 3) * (size / 30);
        const r2 = c - 1.6 * (size / 30);
        return minimal || long || size > 40 ? (
          <line key={i} className="tick" x1={c + Math.sin(a) * r1} y1={c - Math.cos(a) * r1} x2={c + Math.sin(a) * r2} y2={c - Math.cos(a) * r2} />
        ) : null;
      })}
      {hand((h % 12) * 30 + m / 2, c * 0.5, "hour")}
      {hand(m * 6 + s / 10, c * 0.75, "minute")}
      {seconds && hand(s * 6, c * 0.8, "second")}
      <circle cx={c} cy={c} r={Math.max(1.2, size / 30)} className="pin" />
    </svg>
  );
}

export function AnalogMinimalTile({ instance }: WidgetProps) {
  const now = useTick(1000);
  return (
    <div className={`w-analog ${instance.size}`}>
      <Face now={now} size={30} minimal seconds />
      {instance.size !== "s" && (
        <span className="text">
          <b>{new Date(now).toLocaleDateString([], { weekday: "long" })}</b>
          <small>{new Date(now).toLocaleDateString([], { month: "short", day: "numeric" })}</small>
        </span>
      )}
    </div>
  );
}

export function AnalogPanel() {
  const now = useTick(1000);
  return (
    <div className="panel w-clock-panel center">
      <Face now={now} size={150} minimal seconds />
      <div className="big-date">{new Date(now).toLocaleDateString([], { weekday: "long", month: "long", day: "numeric" })}</div>
    </div>
  );
}

export function WorldAnalogTile({ instance }: WidgetProps<ZonesConfig>) {
  const now = useTick(1000);
  const count = { s: 1, m: 2, l: 3 }[instance.size];
  return (
    <div className={`w-worldanalog ${instance.size}`}>
      {zonesOf(instance.config)
        .slice(0, count)
        .map((z) => (
          <span key={z.label + z.tz} className="city" title={`${z.label} ${hhmm(now, local24(), false, z.tz)}`}>
            <Face now={now} tz={z.tz} size={22} />
            <small>{z.label.slice(0, 3).toUpperCase()}</small>
          </span>
        ))}
    </div>
  );
}

export const WorldAnalogPanel = WorldClockPanel;

// ---- World time: one city, large -------------------------------------------------------------

export function WorldTimeTile({ instance }: WidgetProps<ZonesConfig>) {
  const now = useTick(5000);
  const z = zonesOf(instance.config)[0];
  return (
    <div className={`w-worldtime ${instance.size}`} title={`${z.label}, ${utcOffset(z.tz, now)}`}>
      <b className="time">{hhmm(now, local24(), false, z.tz)}</b>
      {instance.size !== "s" && (
        <span className="text">
          <b>{z.label}</b>
          <small>{utcOffset(z.tz, now)}</small>
        </span>
      )}
    </div>
  );
}

export const WorldTimePanel = WorldClockPanel;

// ---- Date and day number ------------------------------------------------------------------

export function DateTile({ instance }: WidgetProps) {
  const now = new Date(useTick(30_000));
  const opts: Intl.DateTimeFormatOptions =
    instance.size === "s" ? { month: "short", day: "numeric" } : instance.size === "m" ? { weekday: "short", month: "short", day: "numeric" } : { weekday: "long", month: "long", day: "numeric" };
  return (
    <div className={`w-date ${instance.size}`}>
      <b>{now.toLocaleDateString([], opts)}</b>
    </div>
  );
}

export function DayNumberTile({ instance }: WidgetProps) {
  const now = new Date(useTick(30_000));
  return (
    <div className={`w-daynum ${instance.size}`}>
      <span className="page">
        <i>{now.toLocaleDateString([], { month: "short" }).toUpperCase()}</i>
        <b>{now.getDate()}</b>
      </span>
      {instance.size !== "s" && (
        <span className="text">
          <b>{now.toLocaleDateString([], { weekday: "long" })}</b>
          {instance.size === "l" && <small>Week {isoWeek(now)}</small>}
        </span>
      )}
    </div>
  );
}

function isoWeek(d: Date) {
  const t = new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()));
  const day = t.getUTCDay() || 7;
  t.setUTCDate(t.getUTCDate() + 4 - day);
  const y0 = new Date(Date.UTC(t.getUTCFullYear(), 0, 1));
  return Math.ceil(((t.getTime() - y0.getTime()) / 864e5 + 1) / 7);
}

export function DatePanel() {
  const now = useTick(30_000);
  const d = new Date(now);
  return (
    <div className="panel w-clock-panel">
      <div className="big-date strong">{d.toLocaleDateString([], { weekday: "long", month: "long", day: "numeric", year: "numeric" })}</div>
      <p className="fine">Week {isoWeek(d)} · day {Math.floor((now - new Date(d.getFullYear(), 0, 1).getTime()) / 864e5) + 1} of the year</p>
      <MonthGrid now={now} />
    </div>
  );
}

// ---- With the weather -------------------------------------------------------------------------

function WeatherPlacePanel({ instance, title, children }: WidgetProps<WeatherConfig> & { title: string; children?: React.ReactNode }) {
  const cfg = instance.config;
  const [editing, setEditing] = useState(!cfg.place);
  return (
    <div className="panel w-clock-panel">
      <h3>
        {title}
        {cfg.place && <span className="now">{cfg.place.name}</span>}
      </h3>
      {children}
      {editing || !cfg.place ? (
        <>
          <p className="fine">Choose a city for the weather.</p>
          <PlacePicker id={instance.id} cfg={cfg} />
        </>
      ) : (
        <div className="actions">
          <button className="link" onClick={() => setEditing(true)}>
            Change city
          </button>
          <button className="link" onClick={() => updateConfig(instance.id, { ...cfg, fahrenheit: !cfg.fahrenheit })}>
            Show °{cfg.fahrenheit ? "C" : "F"}
          </button>
        </div>
      )}
    </div>
  );
}

export function TodayTile({ instance, preview }: WidgetProps<WeatherConfig & ClockConfig>) {
  const now = useTick(1000);
  const { w } = useWeather(instance.config, preview);
  const d = new Date(now);
  return (
    <div className={`w-today ${instance.size}`}>
      <span className="day">
        <small>{d.toLocaleDateString([], { weekday: "short" }).toUpperCase()}</small>
        <b>{d.getDate()}</b>
      </span>
      {instance.size !== "s" && <b className="time">{hhmm(now, is24(instance.config))}</b>}
      {instance.size === "l" && w && (
        <span className="wx">
          {describe(w.code, w.is_day)[0]} {deg(w.temperature)}
        </span>
      )}
    </div>
  );
}

export function TodayPanel(p: WidgetProps<WeatherConfig>) {
  const now = useTick(1000);
  const { w } = useWeather(p.instance.config);
  const d = new Date(now);
  return (
    <WeatherPlacePanel {...p} title="Today">
      <div className="big-time">{hhmm(now, local24())}</div>
      <div className="big-date">{d.toLocaleDateString([], { weekday: "long", month: "long", day: "numeric" })}</div>
      {w && (
        <p className="wx-line">
          {describe(w.code, w.is_day)[0]} {deg(w.temperature)} · {describe(w.code, w.is_day)[1]}
        </p>
      )}
    </WeatherPlacePanel>
  );
}

export function ClockWeatherTile({ instance, preview }: WidgetProps<WeatherConfig & ClockConfig>) {
  const now = useTick(1000);
  const { w } = useWeather(instance.config, preview);
  const d = new Date(now);
  return (
    <div className={`w-blocks ${instance.size}`}>
      <span className="blk b1">{hhmm(now, is24(instance.config))}</span>
      {instance.size !== "s" && <span className="blk b2">{d.toLocaleDateString([], { day: "numeric", month: "short" })}</span>}
      {instance.size === "l" && <span className="blk b3">{w ? deg(w.temperature) : "–"}</span>}
    </div>
  );
}

export const ClockWeatherPanel = (p: WidgetProps<WeatherConfig>) => <TodayPanel {...p} />;

export function SevenDaysTile({ instance, preview }: WidgetProps<WeatherConfig>) {
  const now = new Date(useTick(60_000));
  const { w } = useWeather(instance.config, preview);
  const monday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - ((now.getDay() + 6) % 7));
  const count = instance.size === "s" ? 3 : 7;
  const start = instance.size === "s" ? new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1) : monday;
  return (
    <div className={`w-week ${instance.size}`}>
      {Array.from({ length: count }, (_, i) => {
        const d = new Date(start.getFullYear(), start.getMonth(), start.getDate() + i);
        const today = d.toDateString() === now.toDateString();
        return (
          <span key={i} className={`wd${today ? " today" : ""}`}>
            <small>{d.toLocaleDateString([], { weekday: "narrow" })}</small>
            <b>{d.getDate()}</b>
          </span>
        );
      })}
      {instance.size === "l" && w && <span className="wx">{deg(w.temperature)}</span>}
    </div>
  );
}

export function SevenDaysPanel(p: WidgetProps<WeatherConfig>) {
  const { w } = useWeather(p.instance.config);
  return (
    <WeatherPlacePanel {...p} title="This week">
      {w && (
        <div className="days">
          {w.days.map((d) => (
            <div key={d.date} className="day">
              <span>{new Date(d.date + "T12:00").toLocaleDateString([], { weekday: "short" })}</span>
              <span className="icon">{describe(d.code)[0]}</span>
              <b>{deg(d.max)}</b>
              <small>{deg(d.min)}</small>
            </div>
          ))}
        </div>
      )}
    </WeatherPlacePanel>
  );
}
