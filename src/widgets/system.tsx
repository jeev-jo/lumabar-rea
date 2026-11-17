import { LineChart, Sparkline } from "./Chart";
import { formatRate, useStats, type WidgetProps } from "./shared";

const pct = (v: number) => `${Math.round(v)}%`;

/** Placeholder history for the picker preview. */
const DEMO = Array.from({ length: 60 }, (_, i) => 22 + 14 * Math.sin(i / 6) + 6 * Math.sin(i / 2.3));

export function SystemTile({ instance, preview }: WidgetProps) {
  const samples = useStats(!preview);
  const cpu = preview ? DEMO : samples.map((s) => s.cpu);
  const ram = preview ? DEMO.map((v) => v + 30) : samples.map((s) => s.ram);
  const last = (a: number[]) => (a.length ? a[a.length - 1] : 0);
  const size = instance.size;

  if (size === "s") {
    return (
      <div className="w-system s">
        <span className="label">CPU</span>
        <b>{pct(last(cpu))}</b>
      </div>
    );
  }
  return (
    <div className={`w-system ${size}`}>
      <div className="metric">
        <span className="label">
          CPU <b>{pct(last(cpu))}</b>
        </span>
        <Sparkline values={cpu} max={100} />
      </div>
      {size === "l" && (
        <div className="metric">
          <span className="label">
            RAM <b>{pct(last(ram))}</b>
          </span>
          <Sparkline values={ram} max={100} />
        </div>
      )}
    </div>
  );
}

export function SystemPanel() {
  const samples = useStats(true);
  const last = samples[samples.length - 1];
  return (
    <div className="panel w-system-panel">
      <h3>
        CPU <span className="now">{last ? pct(last.cpu) : "–"}</span>
      </h3>
      <LineChart series={[{ name: "CPU", color: "var(--series-1)", values: samples.map((s) => s.cpu) }]} max={100} format={pct} height={70} />
      <h3>
        Memory{" "}
        <span className="now">
          {last ? `${last.ram_used_gb.toFixed(1)} of ${last.ram_total_gb.toFixed(1)} GB` : "–"}
        </span>
      </h3>
      <LineChart series={[{ name: "Memory", color: "var(--series-1)", values: samples.map((s) => s.ram) }]} max={100} format={pct} height={70} />
      <h3>
        Network <span className="now">{last ? `↓ ${formatRate(last.down)} · ↑ ${formatRate(last.up)}` : "–"}</span>
      </h3>
      <LineChart
        series={[
          { name: "Down", color: "var(--series-1)", values: samples.map((s) => s.down) },
          { name: "Up", color: "var(--series-2)", values: samples.map((s) => s.up) },
        ]}
        format={formatRate}
        height={70}
      />
    </div>
  );
}
