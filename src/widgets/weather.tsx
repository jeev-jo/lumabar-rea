import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { updateConfig, useDebounced, type WidgetProps } from "./shared";

type Place = { name: string; region: string; country: string; latitude: number; longitude: number };
type Day = { date: string; code: number; max: number; min: number };
type Weather = { temperature: number; code: number; is_day: boolean; days: Day[] };
export type WeatherConfig = { place?: Place; fahrenheit?: boolean };

// WMO weather codes, as used by Open-Meteo.
function describe(code: number, day = true): [string, string] {
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
  days: [0, 1, 2, 3, 4].map((i) => ({ date: `2026-11-${17 + i}`, code: [1, 3, 61, 0, 2][i], max: 26 - i, min: 17 - i })),
};

function useWeather(cfg: WeatherConfig, preview?: boolean) {
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
        .then((d) => live && (setW(d), setError(null)))
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

const deg = (v: number) => `${Math.round(v)}°`;

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

function PlacePicker({ id, cfg }: { id: string; cfg: WeatherConfig }) {
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
