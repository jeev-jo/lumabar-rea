import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { updateConfig, useDebounced, type WidgetProps } from "./shared";

type Place = { name: string; region: string; country: string; latitude: number; longitude: number };
type Day = { date: string; code: number; max: number; min: number; sunrise?: string; sunset?: string; rain_chance?: number };
type Hour = { time: string; temperature: number; code: number; rain_chance: number };
export type Weather = {
  temperature: number;
  code: number;
  is_day: boolean;
  feels_like?: number;
  humidity?: number;
  wind?: number;
  wind_unit?: string;
  local_time?: string;
  days: Day[];
  hours?: Hour[];
  /** When this reading arrived (ms), to move the place's clock on. */
  received?: number;
};
export type WeatherConfig = { place?: Place; fahrenheit?: boolean };

// WMO weather codes, as used by Open-Meteo.
export function describe(code: number, day = true): [string, string] {
  if (code === 0) return [day ? "☀️" : "🌙", "Clear"];
  if (code <= 2) return [day ? "🌤️" : "☁️", "Partly cloudy"];
  if (code === 3) return ["☁️", "Overcast"];
  if (code <= 48) return ["🌫️", "Fog"];
  if (code <= 57) return ["🌦️", "Drizzle"];
  if (code <= 67) return ["🌧️", "Rain"];
  if (code <= 77) return ["❄️", "Snow"];
  if (code <= 82) return ["🌧️", "Showers"];
  if (code <= 86) return ["🌨️", "Snow showers"];
  return ["⛈️", "Thunderstorm"];
}

const DEMO: Weather = {
  temperature: 24,
  code: 1,
  is_day: true,
  feels_like: 25,
  humidity: 58,
  wind: 12,
  wind_unit: "km/h",
  local_time: "2026-11-18T14:00",
  received: Date.now(),
  days: [0, 1, 2, 3, 4].map((i) => ({
    date: `2026-11-${18 + i}`,
    code: [1, 3, 61, 0, 2][i],
    max: 26 - i,
    min: 17 - i,
    sunrise: `2026-11-${18 + i}T06:41`,
    sunset: `2026-11-${18 + i}T17:52`,
    rain_chance: [10, 30, 80, 5, 20][i],
  })),
  hours: Array.from({ length: 12 }, (_, i) => ({ time: `2026-11-18T${String(14 + i).padStart(2, "0")}:00`, temperature: 24 - i * 0.6, code: i > 6 ? 3 : 1, rain_chance: i * 3 })),
};

export function useWeather(cfg: WeatherConfig, preview?: boolean) {
  const [w, setW] = useState<Weather | null>(preview ? DEMO : null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (preview || !cfg.place) return;
    let live = true;
    const load = () =>
      invoke<Weather>("weather_get", {
        latitude: cfg.place!.latitude,
        longitude: cfg.place!.longitude,
        fahrenheit: !!cfg.fahrenheit,
      })
        .then((d) => live && (setW({ ...d, received: Date.now() }), setError(null)))
        .catch((e) => live && setError(String(e)));
    load();
    const id = setInterval(load, 15 * 60 * 1000);
    return () => {
      live = false;
      clearInterval(id);
    };
  }, [cfg.place?.latitude, cfg.place?.longitude, cfg.fahrenheit, preview]);
  return { w, error };
}

export const deg = (v: number) => `${Math.round(v)}°`;

export function WeatherTile({ instance, preview }: WidgetProps<WeatherConfig>) {
  const cfg = instance.config;
  const { w, error } = useWeather(cfg, preview);
  if (!preview && !cfg.place) {
    return <div className="w-weather empty">Set city</div>;
  }
  if (!w) return <div className="w-weather empty">{error ? "Offline" : "…"}</div>;
  const [icon, label] = describe(w.code, w.is_day);
  const today = w.days[0];
  return (
    <div className={`w-weather ${instance.size}`} title={`${label}, ${deg(w.temperature)}`}>
      <span className="icon">{icon}</span>
      <span className="temp">{deg(w.temperature)}</span>
      {instance.size !== "s" && (
        <span className="where">
          <b>{preview ? "Your city" : cfg.place!.name}</b>
          {instance.size === "l" && today && (
            <small>
              {deg(today.max)} / {deg(today.min)}
            </small>
          )}
        </span>
      )}
    </div>
  );
}

export function PlacePicker({ id, cfg }: { id: string; cfg: WeatherConfig }) {
  const [q, setQ] = useState("");
  const [results, setResults] = useState<Place[]>([]);
  const search = useDebounced((text: string) => {
    invoke<Place[]>("place_search", { query: text }).then(setResults).catch(() => setResults([]));
  }, 300);
  return (
    <div className="place-picker">
      <input
        autoFocus
        placeholder="Search for a city"
        value={q}
        onChange={(e) => {
          setQ(e.target.value);
          search(e.target.value);
        }}
      />
      <ul>
        {results.map((p) => (
          <li key={`${p.latitude},${p.longitude}`}>
            <button onClick={() => updateConfig(id, { ...cfg, place: p })}>
              <b>{p.name}</b> {[p.region, p.country].filter(Boolean).join(", ")}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

export function WeatherPanel({ instance }: WidgetProps<WeatherConfig>) {
  const cfg = instance.config;
  const [editing, setEditing] = useState(!cfg.place);
  const { w, error } = useWeather(cfg);

  if (editing || !cfg.place) {
    return (
      <div className="panel w-weather-panel">
        <h3>Weather location</h3>
        <PlacePicker id={instance.id} cfg={cfg} />
        {cfg.place && (
          <button className="link" onClick={() => setEditing(false)}>
            Cancel
          </button>
        )}
        <p className="fine">Only the city's coordinates are sent, to Open-Meteo.</p>
      </div>
    );
  }
  return (
    <div className="panel w-weather-panel">
      <h3>
        {cfg.place.name}
        <span className="now">{[cfg.place.region, cfg.place.country].filter(Boolean).join(", ")}</span>
      </h3>
      {error && <p className="error">{error}</p>}
      {w && (
        <>
          <div className="current">
            <span className="icon">{describe(w.code, w.is_day)[0]}</span>
            <span className="temp">{deg(w.temperature)}</span>
            <span>{describe(w.code, w.is_day)[1]}</span>
          </div>
          {w.wind !== undefined && (
            <div className="wx-facts">
              <span>
                <small>Feels like</small>
                <b>{deg(w.feels_like ?? w.temperature)}</b>
              </span>
              <span>
                <small>Wind</small>
                <b>
                  {Math.round(w.wind)} {w.wind_unit}
                </b>
              </span>
              <span>
                <small>Humidity</small>
                <b>{Math.round(w.humidity ?? 0)}%</b>
              </span>
              <span>
                <small>Rain</small>
                <b>{Math.round(w.days[0]?.rain_chance ?? 0)}%</b>
              </span>
            </div>
          )}
          {!!w.hours?.length && (
            <div className="hours">
              {w.hours.slice(0, 8).map((h) => (
                <div key={h.time} className="hour">
                  <small>{hourLabel(h.time)}</small>
                  <span className="icon">{describe(h.code)[0]}</span>
                  <b>{deg(h.temperature)}</b>
                </div>
              ))}
            </div>
          )}
          {w.days[0]?.sunrise && (
            <p className="fine">
              Sunrise {clock(w.days[0].sunrise)} · Sunset {clock(w.days[0].sunset!)}
            </p>
          )}
          <div className="days">
            {w.days.slice(0, 5).map((d) => (
              <div key={d.date} className="day">
                <span>{new Date(d.date + "T12:00").toLocaleDateString([], { weekday: "short" })}</span>
                <span className="icon">{describe(d.code)[0]}</span>
                <b>{deg(d.max)}</b>
                <small>{deg(d.min)}</small>
              </div>
            ))}
          </div>
        </>
      )}
      <div className="actions">
        <button className="link" onClick={() => setEditing(true)}>
          Change city
        </button>
        <button className="link" onClick={() => updateConfig(instance.id, { ...cfg, fahrenheit: !cfg.fahrenheit })}>
          Show °{cfg.fahrenheit ? "C" : "F"}
        </button>
      </div>
    </div>
  );
}

// ---- More weather tiles ----------------------------------------------------------

/** "2 PM" or "14:00" for "2026-11-18T14:00", in this PC's 12/24-hour style. */
function clock(t: string) {
  const [h, m] = t.slice(11, 16).split(":").map(Number);
  return new Date(2000, 0, 1, h, m).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

function hourLabel(t: string) {
  const h = Number(t.slice(11, 13));
  return new Date(2000, 0, 1, h).toLocaleTimeString([], { hour: "numeric" });
}

/** The tile to show instead while there's no city or no reading yet. */
function gate(p: WidgetProps<WeatherConfig>, w: Weather | null, error: string | null) {
  if (!p.preview && !p.instance.config.place) return <div className="w-weather empty">Set city</div>;
  if (!w) return <div className="w-weather empty">{error ? "Offline" : "…"}</div>;
  return null;
}

const G_WIND = "💨";
const G_RAIN = "💧";

export function WeatherDetailsTile(p: WidgetProps<WeatherConfig>) {
  const { w, error } = useWeather(p.instance.config, p.preview);
  if (!w || (!p.preview && !p.instance.config.place)) return gate(p, w, error);
  const today = w.days[0];
  return (
    <div className={`w-wxd ${p.instance.size}`} title={`Wind ${Math.round(w.wind ?? 0)} ${w.wind_unit}, rain ${Math.round(today?.rain_chance ?? 0)}%`}>
      <span className="fact">
        <span className="emoji">{G_WIND}</span>
        <b>{Math.round(w.wind ?? 0)}</b>
        {p.instance.size === "l" && <small>{w.wind_unit}</small>}
      </span>
      {p.instance.size !== "s" && (
        <span className="fact">
          <span className="emoji">{G_RAIN}</span>
          <b>{Math.round(today?.rain_chance ?? 0)}%</b>
        </span>
      )}
      {p.instance.size === "l" && today && (
        <span className="fact">
          <b>{deg(today.max)}</b>
          <small>{deg(today.min)}</small>
        </span>
      )}
    </div>
  );
}

export function TodayForecastTile(p: WidgetProps<WeatherConfig>) {
  const { w, error } = useWeather(p.instance.config, p.preview);
  if (!w || (!p.preview && !p.instance.config.place)) return gate(p, w, error);
  const next = w.days.slice(1, p.instance.size === "l" ? 4 : 2);
  return (
    <div className={`w-fc ${p.instance.size}`}>
      <span className="now">
        <span className="icon">{describe(w.code, w.is_day)[0]}</span>
        <b>{deg(w.temperature)}</b>
      </span>
      {p.instance.size !== "s" &&
        next.map((d) => (
          <span key={d.date} className="d">
            <small>{new Date(d.date + "T12:00").toLocaleDateString([], { weekday: "short" })}</small>
            <span>
              {describe(d.code)[0]}
              {deg(d.max)}
            </span>
          </span>
        ))}
    </div>
  );
}

export function WeatherStatusTile(p: WidgetProps<WeatherConfig>) {
  const { w, error } = useWeather(p.instance.config, p.preview);
  if (!w || (!p.preview && !p.instance.config.place)) return gate(p, w, error);
  const [icon, words] = describe(w.code, w.is_day);
  return (
    <div className={`w-wxs ${p.instance.size}`} title={`${words}, ${deg(w.temperature)}`}>
      {p.instance.size === "l" && <span className="icon">{icon}</span>}
      <span className="text">
        <b>{words}</b>
        {p.instance.size !== "s" && <small>{deg(w.temperature)}, feels {deg(w.feels_like ?? w.temperature)}</small>}
      </span>
    </div>
  );
}

export function HourlyTile(p: WidgetProps<WeatherConfig>) {
  const { w, error } = useWeather(p.instance.config, p.preview);
  if (!w || (!p.preview && !p.instance.config.place)) return gate(p, w, error);
  const n = { s: 2, m: 3, l: 5 }[p.instance.size];
  return (
    <div className={`w-hourly ${p.instance.size}`}>
      {(w.hours ?? []).slice(0, n).map((h, i) => (
        <span key={h.time} className="h">
          <small>{i === 0 ? "Now" : hourLabel(h.time)}</small>
          <b>{deg(h.temperature)}</b>
        </span>
      ))}
    </div>
  );
}

/** Where the sun is between today's sunrise and sunset, at the place. */
function sunNow(w: Weather) {
  const d = w.days[0];
  if (!d?.sunrise || !d.sunset || !w.local_time) return null;
  const at = (t: string) => Date.parse(t + ":00Z");
  const now = at(w.local_time) + (Date.now() - (w.received ?? Date.now()));
  const rise = at(d.sunrise);
  const set = at(d.sunset);
  const f = (now - rise) / (set - rise);
  const next = now < rise ? d.sunrise : now < set ? d.sunset : (w.days[1]?.sunrise ?? d.sunrise);
  return { f, rise: d.sunrise, set: d.sunset, next, nextIsSet: next === d.sunset };
}

function SunArc({ f, width, height }: { f: number; width: number; height: number }) {
  const c = Math.max(0, Math.min(1, f));
  const x = 3 + c * (width - 6);
  const y = height - 3 - Math.sin(c * Math.PI) * (height - 8);
  return (
    <svg className="sunarc" width={width} height={height} viewBox={`0 0 ${width} ${height}`} aria-hidden>
      <path d={`M3 ${height - 3} Q ${width / 2} ${-height + 13} ${width - 3} ${height - 3}`} className="path" />
      <line x1="0" x2={width} y1={height - 3} y2={height - 3} className="horizon" />
      {f >= 0 && f <= 1 && <circle cx={x} cy={y} r="3.5" className="sun" />}
    </svg>
  );
}

export function SunTile(p: WidgetProps<WeatherConfig>) {
  const { w, error } = useWeather(p.instance.config, p.preview);
  if (!w || (!p.preview && !p.instance.config.place)) return gate(p, w, error);
  const s = sunNow(w);
  if (!s) return <div className="w-weather empty">…</div>;
  return (
    <div className={`w-sun ${p.instance.size}`} title={`Sunrise ${clock(s.rise)}, sunset ${clock(s.set)}`}>
      {p.instance.size !== "s" && <SunArc f={s.f} width={p.instance.size === "l" ? 76 : 40} height={26} />}
      <span className="text">
        <small>{s.nextIsSet ? "Sunset" : "Sunrise"}</small>
        <b>{clock(s.next)}</b>
      </span>
    </div>
  );
}
