import { useMemo, useState } from "react";
import { updateConfig, useTick, type WidgetProps } from "./shared";

type Zone = { label: string; tz: string };
export type WorldClockConfig = { zones?: Zone[] };

const DEFAULT_ZONES: Zone[] = [
  { label: "London", tz: "Europe/London" },
  { label: "New York", tz: "America/New_York" },
  { label: "Tokyo", tz: "Asia/Tokyo" },
];

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
          <span key={z.tz} className="zone">
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
  const all = useMemo(() => (Intl as unknown as { supportedValuesOf(k: string): string[] }).supportedValuesOf("timeZone"), []);
  const matches = q.trim().length < 2 ? [] : all.filter((tz) => tz.toLowerCase().replace(/_/g, " ").includes(q.trim().toLowerCase())).slice(0, 6);
  const save = (next: Zone[]) => updateConfig(instance.id, { ...instance.config, zones: next });

  return (
    <div className="panel w-worldclock-panel">
      <h3>World clock</h3>
      <ul className="zones">
        {zones.map((z, i) => (
          <li key={z.tz}>
            <span className="name">
              <b>{z.label}</b>
              <small>{offset(z.tz, now)}</small>
            </span>
            <span className="t">{time(z.tz, now)}</span>
            <button className="remove" title="Remove" disabled={zones.length === 1} onClick={() => save(zones.filter((_, j) => j !== i))}>
              ×
            </button>
          </li>
        ))}
      </ul>
      <input placeholder="Add a city or time zone (e.g. Paris)" value={q} onChange={(e) => setQ(e.target.value)} />
      <ul className="results">
        {matches.map((tz) => (
          <li key={tz}>
            <button
              onClick={() => {
                const label = tz.split("/").pop()!.replace(/_/g, " ");
                if (!zones.some((z) => z.tz === tz)) save([...zones, { label, tz }]);
                setQ("");
              }}
            >
              {tz.replace(/_/g, " ")}
            </button>
          </li>
        ))}
      </ul>
      <p className="fine">The dock shows the first {instance.size === "s" ? "one" : instance.size === "m" ? "two" : "three"} of these.</p>
    </div>
  );
}
