import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type Status = {
  volume: { level: number; muted: boolean } | null;
  network: "wifi" | "ethernet" | "other" | "offline";
  battery: { percent: number; charging: boolean } | null;
  media: { title: string; artist: string; playing: boolean } | null;
};

// Glyphs from Segoe Fluent Icons, which ships with Windows 11.
const G = {
  wifi: "",
  ethernet: "",
  other: "", // globe
  offline: "",
  mute: "",
  vol: ["", "", "", ""],
  play: "",
  pause: "",
  prev: "",
  next: "",
  bolt: "",
};

const flyout = (which: string) => invoke("open_flyout", { which }).catch(console.error);

export function useSystemStatus() {
  const [status, setStatus] = useState<Status | null>(null);
  useEffect(() => {
    invoke<Status>("system_status").then(setStatus).catch(console.error);
    const un = listen<Status>("system-status", (e) => setStatus(e.payload));
    return () => {
      un.then((f) => f());
    };
  }, []);
  return status;
}

export function StartButton() {
  return (
    <button className="sys start" title="Start" onClick={() => flyout("start")}>
      <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden>
        <circle cx="12" cy="12" r="8.5" fill="none" stroke="currentColor" strokeWidth="2" />
        <circle cx="12" cy="12" r="3.2" fill="currentColor" />
      </svg>
    </button>
  );
}

function volumeGlyph(v: Status["volume"]) {
  if (!v || v.muted || v.level === 0) return G.mute;
  return G.vol[v.level < 34 ? 1 : v.level < 67 ? 2 : 3];
}

function MediaControls({ media }: { media: NonNullable<Status["media"]> }) {
  const control = (action: string) => invoke("media_control", { action }).catch(console.error);
  const text = media.artist ? `${media.title} — ${media.artist}` : media.title;
  return (
    <div className="media" title={text}>
      <button className="sys glyph small" title="Previous" onClick={() => control("previous")}>
        {G.prev}
      </button>
      <button className="sys glyph" title={media.playing ? "Pause" : "Play"} onClick={() => control("toggle")}>
        {media.playing ? G.pause : G.play}
      </button>
      <button className="sys glyph small" title="Next" onClick={() => control("next")}>
        {G.next}
      </button>
      <span className="media-text">
        <b>{media.title}</b>
        {media.artist && <small>{media.artist}</small>}
      </span>
    </div>
  );
}

function BatteryIcon({ b }: { b: NonNullable<Status["battery"]> }) {
  const low = b.percent <= 15 && !b.charging;
  return (
    <span className={`battery${low ? " low" : ""}`}>
      <span className="cell">
        <span className="fill" style={{ width: `${b.percent}%` }} />
      </span>
      {b.charging && <span className="glyph bolt">{G.bolt}</span>}
    </span>
  );
}

const networkLabel = {
  wifi: "Wi-Fi",
  ethernet: "Ethernet",
  other: "Connected",
  offline: "No internet",
};

export function SystemStrip({ status }: { status: Status | null }) {
  if (!status) return null;
  const v = status.volume;
  return (
    <div className="system">
      {status.media && <MediaControls media={status.media} />}
      <button
        className="sys tray"
        title={`${networkLabel[status.network]}${v ? ` · Volume ${v.muted ? "muted" : v.level + "%"}` : ""}${
          status.battery ? ` · Battery ${status.battery.percent}%${status.battery.charging ? " (charging)" : ""}` : ""
        }`}
        onClick={() => flyout("quick")}
        // Scroll anywhere on the status icons to change the volume; middle-click mutes.
        onWheel={(e) => invoke("volume_step", { delta: e.deltaY < 0 ? 2 : -2 }).catch(console.error)}
        onMouseDown={(e) => e.button === 1 && e.preventDefault()}
        onAuxClick={(e) => e.button === 1 && invoke("volume_toggle_mute").catch(console.error)}
      >
        <span className="glyph">{G[status.network]}</span>
        <span className="glyph">{volumeGlyph(v)}</span>
        {status.battery && <BatteryIcon b={status.battery} />}
      </button>
    </div>
  );
}

export function Clock({ now }: { now: Date }) {
  return (
    <button className="sys clock" title={now.toLocaleString()} onClick={() => flyout("notifications")}>
      {now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
      <small>{now.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" })}</small>
    </button>
  );
}
