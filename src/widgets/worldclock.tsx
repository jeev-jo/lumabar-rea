import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { updateConfig, useDebounced, useTick, type WidgetProps } from "./shared";

type Zone = { label: string; tz: string };
export type WorldClockConfig = { zones?: Zone[] };

const DEFAULT_ZONES: Zone[] = [
  { label: "London", tz: "Europe/London" },
  { label: "New York", tz: "America/New_York" },
  { label: "Tokyo", tz: "Asia/Tokyo" },
];

/** Countries with more than one time zone: searching the country lists each one. */
const COUNTRY_ZONES: Record<string, [label: string, tz: string][]> = {
  Canada: [
    ["Vancouver", "America/Vancouver"], ["Edmonton", "America/Edmonton"], ["Regina", "America/Regina"],
    ["Winnipeg", "America/Winnipeg"], ["Toronto", "America/Toronto"], ["Halifax", "America/Halifax"],
    ["St. John's", "America/St_Johns"],
  ],
  "United States": [
    ["Honolulu", "Pacific/Honolulu"], ["Anchorage", "America/Anchorage"], ["Los Angeles", "America/Los_Angeles"],
    ["Denver", "America/Denver"], ["Phoenix", "America/Phoenix"], ["Chicago", "America/Chicago"], ["New York", "America/New_York"],
  ],
  Australia: [
    ["Perth", "Australia/Perth"], ["Darwin", "Australia/Darwin"], ["Adelaide", "Australia/Adelaide"],
    ["Brisbane", "Australia/Brisbane"], ["Sydney", "Australia/Sydney"], ["Hobart", "Australia/Hobart"],
  ],
  Russia: [
    ["Kaliningrad", "Europe/Kaliningrad"], ["Moscow", "Europe/Moscow"], ["Samara", "Europe/Samara"],
    ["Yekaterinburg", "Asia/Yekaterinburg"], ["Omsk", "Asia/Omsk"], ["Novosibirsk", "Asia/Novosibirsk"],
    ["Krasnoyarsk", "Asia/Krasnoyarsk"], ["Irkutsk", "Asia/Irkutsk"], ["Yakutsk", "Asia/Yakutsk"],
    ["Vladivostok", "Asia/Vladivostok"], ["Magadan", "Asia/Magadan"], ["Kamchatka", "Asia/Kamchatka"],
  ],
  Brazil: [["Rio Branco", "America/Rio_Branco"], ["Manaus", "America/Manaus"], ["São Paulo", "America/Sao_Paulo"], ["Fernando de Noronha", "America/Noronha"]],
  Mexico: [["Tijuana", "America/Tijuana"], ["Hermosillo", "America/Hermosillo"], ["Mexico City", "America/Mexico_City"], ["Cancún", "America/Cancun"]],
  Indonesia: [["Jakarta", "Asia/Jakarta"], ["Makassar", "Asia/Makassar"], ["Jayapura", "Asia/Jayapura"]],
  Kazakhstan: [["Aqtobe", "Asia/Aqtobe"], ["Almaty", "Asia/Almaty"]],
  Mongolia: [["Hovd", "Asia/Hovd"], ["Ulaanbaatar", "Asia/Ulaanbaatar"]],
  Chile: [["Santiago", "America/Santiago"], ["Easter Island", "Pacific/Easter"]],
  Spain: [["Madrid", "Europe/Madrid"], ["Canary Islands", "Atlantic/Canary"]],
  Portugal: [["Lisbon", "Europe/Lisbon"], ["Azores", "Atlantic/Azores"]],
  "New Zealand": [["Auckland", "Pacific/Auckland"], ["Chatham Islands", "Pacific/Chatham"]],
};

type Hit = { label: string; tz: string; where: string };
type Place = { name: string; region: string; country: string; timezone?: string };

const zonesOf = (cfg: WorldClockConfig) => (cfg.zones?.length ? cfg.zones : DEFAULT_ZONES);
const time = (tz: string, now: number) => new Date(now).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", timeZone: tz });

function offset(tz: string, now: number) {
  // Hours ahead of / behind this PC's time zone.
  const here = new Date(new Date(now).toLocaleString("en-US"));
  const there = new Date(new Date(now).toLocaleString("en-US", { timeZone: tz }));
  const h = Math.round((there.getTime() - here.getTime()) / 36e5 * 2) / 2;
  return h === 0 ? "same time" : `${h > 0 ? "+" : ""}${h}h`;
}

export function WorldClockTile({ instance }: WidgetProps<WorldClockConfig>) {
  const now = useTick(15_000);
  const count = { s: 1, m: 2, l: 3 }[instance.size];
  return (
    <div className={`w-worldclock ${instance.size}`}>
      {zonesOf(instance.config)
        .slice(0, count)
        .map((z) => (
          <span key={z.label + z.tz} className="zone">
            <small>{z.label}</small>
            <b>{time(z.tz, now)}</b>
          </span>
        ))}
    </div>
  );
}

export function WorldClockPanel({ instance }: WidgetProps<WorldClockConfig>) {
  const now = useTick(15_000);
  const zones = zonesOf(instance.config);
  const [q, setQ] = useState("");
  const [places, setPlaces] = useState<Place[]>([]);
  const all = useMemo(() => (Intl as unknown as { supportedValuesOf(k: string): string[] }).supportedValuesOf("timeZone"), []);
  const lookup = useDebounced((text: string) => {
    invoke<Place[]>("place_search", { query: text }).then(setPlaces).catch(() => setPlaces([]));
  }, 300);
  // Countries with several zones, then cities and countries from the place
  // search (any town, with its time zone), then raw time-zone names.
  const matches = useMemo(() => {
    const t = q.trim().toLowerCase();
    if (t.length < 2) return [];
    const hits: Hit[] = [];
    for (const [country, list] of Object.entries(COUNTRY_ZONES))
      if (country.toLowerCase().startsWith(t)) hits.push(...list.map(([label, tz]) => ({ label, tz, where: country })));
    for (const p of places)
      if (p.timezone)
        hits.push(
          // A whole country ("Germany") is named by its time zone's city (Berlin).
          p.name === p.country
            ? { label: p.timezone.split("/").pop()!.replace(/_/g, " "), tz: p.timezone, where: p.country }
            : { label: p.name, tz: p.timezone, where: [p.region, p.country].filter((x) => x && x !== p.name).join(", ") },
        );
    for (const tz of all)
      if (tz.toLowerCase().replace(/_/g, " ").includes(t)) hits.push({ label: tz.split("/").pop()!.replace(/_/g, " "), tz, where: tz.replace(/_/g, " ") });
    const seen = new Set<string>();
    return hits.filter((h) => !seen.has(h.label + h.tz) && seen.add(h.label + h.tz)).slice(0, 10);
  }, [q, places, all]);
  const save = (next: Zone[]) => updateConfig(instance.id, { ...instance.config, zones: next });
  // Reorder by dragging a row, or with its up and down arrows; the dock shows the first ones.
  const [dragFrom, setDragFrom] = useState<number | null>(null);
  const [dragOver, setDragOver] = useState<number | null>(null);
  const move = (from: number, to: number) => {
    if (to < 0 || to >= zones.length || from === to) return;
    const next = [...zones];
    next.splice(to, 0, ...next.splice(from, 1));
    save(next);
  };

  return (
    <div className="panel w-worldclock-panel">
      <h3>World clock</h3>
      <ul className="zones">
        {zones.map((z, i) => (
          <li
            key={z.label + z.tz}
            draggable
            className={dragOver === i && dragFrom !== null && dragFrom !== i ? (dragFrom < i ? "drop-after" : "drop-before") : ""}
            onDragStart={(e) => {
              setDragFrom(i);
              e.dataTransfer.effectAllowed = "move";
            }}
            onDragOver={(e) => {
              e.preventDefault();
              setDragOver(i);
            }}
            onDragEnd={() => (setDragFrom(null), setDragOver(null))}
            onDrop={(e) => {
              e.preventDefault();
              if (dragFrom !== null) move(dragFrom, i);
              setDragFrom(null);
              setDragOver(null);
            }}
          >
            <span className="grip" aria-hidden="true">⠿</span>
            <span className="name">
              <b>{z.label}</b>
              <small>{offset(z.tz, now)}</small>
            </span>
            <span className="t">{time(z.tz, now)}</span>
            <span className="order">
              <button title="Move up" aria-label={`Move ${z.label} up`} disabled={i === 0} onClick={() => move(i, i - 1)}>
                ▲
              </button>
              <button title="Move down" aria-label={`Move ${z.label} down`} disabled={i === zones.length - 1} onClick={() => move(i, i + 1)}>
                ▼
              </button>
            </span>
            <button className="remove" title="Remove" disabled={zones.length === 1} onClick={() => save(zones.filter((_, j) => j !== i))}>
              ×
            </button>
          </li>
        ))}
      </ul>
      <input
        placeholder="Add a city or country (e.g. Toronto, Germany)"
        value={q}
        onChange={(e) => {
          setQ(e.target.value);
          setPlaces([]);
          lookup(e.target.value.trim());
        }}
      />
      <ul className="results">
        {matches.map((h) => (
          <li key={h.label + h.tz}>
            <button
              onClick={() => {
                if (!zones.some((z) => z.tz === h.tz && z.label === h.label)) save([...zones, { label: h.label, tz: h.tz }]);
                setQ("");
                setPlaces([]);
              }}
            >
              <b>{h.label}</b> <small>{[h.where, offset(h.tz, now)].filter(Boolean).join(" · ")}</small>
            </button>
          </li>
        ))}
      </ul>
      <p className="fine">The dock shows the first {instance.size === "s" ? "one" : instance.size === "m" ? "two" : "three"} of these.</p>
    </div>
  );
}
