import { useState } from "react";

/**
 * Small charts for the system monitor.
 * Follows the dataviz rules: one y-axis, 2px lines, recessive grid, a legend for
 * two or more series (plus direct end labels), crosshair + tooltip on hover,
 * text in ink colours (never the series colour).
 */

type Series = { name: string; color: string; values: number[] };

export function Sparkline({ values, max, color = "var(--series-1)" }: { values: number[]; max: number; color?: string }) {
  const w = 100;
  const h = 30;
  if (values.length < 2) return <svg className="sparkline" viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" />;
  const step = w / (60 - 1);
  const start = w - (values.length - 1) * step;
  const pts = values.map((v, i) => `${(start + i * step).toFixed(2)},${(h - 2 - (Math.min(v, max) / max) * (h - 4)).toFixed(2)}`);
  return (
    <svg className="sparkline" viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" aria-hidden>
      <polyline points={pts.join(" ")} fill="none" stroke={color} strokeWidth="2" vectorEffect="non-scaling-stroke" strokeLinejoin="round" strokeLinecap="round" />
    </svg>
  );
}

/** End-of-line label positions, pushed apart so two lines ending close together don't overlap. */
function endLabels(series: Series[], y: (v: number) => number, minY: number, maxY: number) {
  const GAP = 11;
  const labels = series
    .filter((s) => s.values.length)
    .map((s) => ({ name: s.name, y: y(s.values[s.values.length - 1]) }))
    .sort((a, b) => a.y - b.y);
  for (let i = 1; i < labels.length; i++) {
    labels[i].y = Math.max(labels[i].y, labels[i - 1].y + GAP);
  }
  // If that ran past the bottom, shift the whole stack up.
  const over = labels.length ? labels[labels.length - 1].y - maxY : 0;
  if (over > 0) labels.forEach((l) => (l.y = Math.max(minY, l.y - over)));
  return labels;
}

function niceMax(v: number) {
  if (v <= 0) return 1;
  const p = Math.pow(10, Math.floor(Math.log10(v)));
  const n = v / p;
  return (n <= 1 ? 1 : n <= 2 ? 2 : n <= 5 ? 5 : 10) * p;
}

export function LineChart({
  series,
  max,
  format,
  height = 96,
}: {
  series: Series[];
  /** Fixed top of the y-axis; omitted = scale to the data. */
  max?: number;
  format: (v: number) => string;
  height?: number;
}) {
  const [hover, setHover] = useState<number | null>(null);
  const W = 300;
  const H = height;
  const padL = 44;
  const padR = series.length > 1 ? 40 : 8;
  const padT = 6;
  const padB = 6;
  const n = 60;
  const top = max ?? niceMax(Math.max(1, ...series.flatMap((s) => s.values)));
  const x = (i: number, len: number) => padL + ((n - len + i) / (n - 1)) * (W - padL - padR);
  const y = (v: number) => padT + (1 - Math.min(v, top) / top) * (H - padT - padB);
  const len = Math.max(0, ...series.map((s) => s.values.length));
  const grid = [0, 0.5, 1].map((f) => top * f);

  function onMove(e: React.MouseEvent<SVGSVGElement>) {
    const r = e.currentTarget.getBoundingClientRect();
    const px = ((e.clientX - r.left) / r.width) * W;
    if (len < 1) return;
    const i = Math.round(((px - padL) / (W - padL - padR)) * (n - 1)) - (n - len);
    setHover(i >= 0 && i < len ? i : null);
  }

  return (
    <div className="chart">
      {series.length > 1 && (
        <div className="legend">
          {series.map((s) => (
            <span key={s.name}>
              <i style={{ background: s.color }} />
              {s.name}
            </span>
          ))}
        </div>
      )}
      <svg viewBox={`0 0 ${W} ${H}`} onMouseMove={onMove} onMouseLeave={() => setHover(null)} role="img">
        {grid.map((g) => (
          <g key={g}>
            <line x1={padL} x2={W - padR} y1={y(g)} y2={y(g)} className="grid" />
            <text x={padL - 6} y={y(g)} className="axis" textAnchor="end" dominantBaseline="middle">
              {format(g)}
            </text>
          </g>
        ))}
        {series.map((s) =>
          s.values.length > 1 ? (
            <polyline
              key={s.name}
              points={s.values.map((v, i) => `${x(i, s.values.length)},${y(v)}`).join(" ")}
              fill="none"
              stroke={s.color}
              strokeWidth="2"
              strokeLinejoin="round"
              strokeLinecap="round"
            />
          ) : null,
        )}
        {/* Direct labels at the line ends, alongside the legend, when there are 2+ series. */}
        {series.length > 1 &&
          endLabels(series, y, padT, H - padB).map((l) => (
            <text key={l.name} x={W - padR + 4} y={l.y} className="axis" dominantBaseline="middle">
              {l.name}
            </text>
          ))}
        {hover !== null && (
          <g>
            <line x1={x(hover, len)} x2={x(hover, len)} y1={padT} y2={H - padB} className="crosshair" />
            {series.map((s) =>
              s.values[hover] !== undefined ? (
                <circle key={s.name} cx={x(hover, s.values.length)} cy={y(s.values[hover])} r="4" fill={s.color} className="dot" />
              ) : null,
            )}
          </g>
        )}
      </svg>
      {hover !== null && (
        <div className="tooltip" style={{ left: `${(x(hover, len) / W) * 100}%` }}>
          <div className="when">{len - 1 - hover === 0 ? "now" : `${len - 1 - hover}s ago`}</div>
          {series.map((s) =>
            s.values[hover] !== undefined ? (
              <div key={s.name} className="val">
                <i style={{ background: s.color }} />
                {series.length > 1 && <span>{s.name}</span>}
                <b>{format(s.values[hover])}</b>
              </div>
            ) : null,
          )}
        </div>
      )}
    </div>
  );
}
