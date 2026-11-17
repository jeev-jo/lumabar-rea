import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Status = {
  volume: { level: number; muted: boolean; device: string } | null;
  network: "wifi" | "ethernet" | "other" | "offline";
  battery: { percent: number; charging: boolean; minutes_left: number | null } | null;
  media: { title: string; artist: string; playing: boolean; art: string | null } | null;
  wifi: boolean | null;
  bluetooth: boolean | null;
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
      {/* The Windows logo, as on the Windows 11 Start button. */}
      <svg viewBox="0 0 20 20" width="19" height="19" aria-hidden>
        <defs>
          <linearGradient id="start-blue" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0" stopColor="#28a8ea" />
            <stop offset="1" stopColor="#0f6cbd" />
          </linearGradient>
        </defs>
        <g fill="url(#start-blue)">
          <rect x="0.5" y="0.5" width="9" height="9" rx="0.8" />
          <rect x="10.5" y="0.5" width="9" height="9" rx="0.8" />
          <rect x="0.5" y="10.5" width="9" height="9" rx="0.8" />
          <rect x="10.5" y="10.5" width="9" height="9" rx="0.8" />
        </g>
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
  // Inner cell is 17 units wide; keep a sliver visible so "almost empty" still shows.
  const fill = Math.max(1.2, (17 * Math.min(100, b.percent)) / 100);
  return (
    <svg
      className={`battery${low ? " low" : ""}${b.charging ? " charging" : ""}`}
      viewBox="0 0 26 14"
      width="26"
      height="14"
      aria-hidden
    >
      <rect className="shell" x="1" y="1" width="21" height="12" rx="3.2" />
      <rect className="nub" x="23" y="4.5" width="2" height="5" rx="1" />
      <rect className="level" x="3" y="3" width={fill} height="8" rx="1.6" />
      {b.charging && <path className="bolt" d="M13.4 1.9 L8.2 7.7 H11.4 L10 12.1 L15.2 6.3 H12 Z" />}
    </svg>
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
