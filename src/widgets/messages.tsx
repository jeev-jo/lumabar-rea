/**
 * WhatsApp: messages waiting in Windows' notification center, so the count
 * and who wrote last show on the dock. Read from Windows itself; nothing is
 * sent anywhere, and clearing the notifications clears the count.
 */
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ServiceIcon } from "./ai";
import type { WidgetProps } from "./shared";

type Messages = { known: boolean; count: number; items: { title: string; text: string; at: number }[] };

const DEMO: Messages = {
  known: true,
  count: 3,
  items: [
    { title: "Priya", text: "Running 10 minutes late", at: Date.now() - 4 * 60_000 },
    { title: "Family", text: "Photos from today", at: Date.now() - 40 * 60_000 },
    { title: "Sam", text: "Sounds good!", at: Date.now() - 3 * 3600_000 },
  ],
};

const ago = (t: number) => {
  const m = Math.max(0, Math.round((Date.now() - t) / 60_000));
  if (m < 1) return "now";
  if (m < 60) return `${m}m`;
  if (m < 24 * 60) return `${Math.round(m / 60)}h`;
  return `${Math.round(m / 1440)}d`;
};

const load = () => invoke<Messages>("app_messages", { app: "whatsapp" });

export function WhatsAppTile({ instance, preview }: WidgetProps) {
  const [m] = usePollMessages(preview);
  const latest = m?.items[0];
  return (
    <div className={`w-msgs ${instance.size}`} title={m ? `${m.count} WhatsApp message${m.count === 1 ? "" : "s"} waiting` : "WhatsApp"}>
      <span className="icon-wrap">
        <ServiceIcon service="whatsapp" label="WhatsApp" size={instance.size === "s" ? 20 : 24} />
        {m && m.count > 0 && <span className="badge">{m.count > 99 ? "99+" : m.count}</span>}
      </span>
      {instance.size !== "s" && (
        <span className="msg-text">
          <b>{m?.count ? latest?.title || "WhatsApp" : "WhatsApp"}</b>
          <small>{m === null ? "…" : m.count ? (instance.size === "l" ? latest?.text || "New message" : `${m.count} unread`) : "No new messages"}</small>
        </span>
      )}
    </div>
  );
}

export function WhatsAppPanel() {
  const [m, , reload] = usePollMessages();
  return (
    <div className="panel w-msgs-panel">
      <div className="svc-head">
        <ServiceIcon service="whatsapp" label="WhatsApp" size={28} />
        <div>
          <h3>WhatsApp</h3>
          <span className="fine">{m ? `${m.count} message${m.count === 1 ? "" : "s"} waiting` : "Reading…"}</span>
        </div>
      </div>
      {m && m.count > 0 && (
        <ul className="msg-list">
          {m.items.slice(0, 8).map((it, i) => (
            <li key={i}>
              <span className="who">{it.title || "WhatsApp"}</span>
              <span className="when">{ago(it.at)}</span>
              {it.text && <span className="what">{it.text}</span>}
            </li>
          ))}
        </ul>
      )}
      {m && m.count === 0 && <p className="fine">Nothing waiting.</p>}
      <div className="row-buttons">
        <button className="primary" onClick={() => invoke("open_link", { url: "whatsapp://" }).catch(() => invoke("open_link", { url: "https://web.whatsapp.com" }))}>
          Open WhatsApp
        </button>
        <button onClick={() => reload()}>Refresh</button>
      </div>
      <p className="fine">
        {m && !m.known
          ? "WhatsApp hasn't sent a notification on this PC yet. Install the WhatsApp app and keep its notifications on."
          : "Counted from WhatsApp's notifications in Windows' notification center. Clearing them there clears the count. Nothing is sent anywhere."}
      </p>
    </div>
  );
}

/** Re-read every 20 seconds while shown. */
function usePollMessages(preview?: boolean) {
  const [value, setValue] = useState<Messages | null>(preview ? DEMO : null);
  const reload = useCallback(() => {
    if (!preview) load().then(setValue).catch(() => {});
  }, [preview]);
  useEffect(() => {
    if (preview) return;
    reload();
    const id = setInterval(reload, 20_000);
    return () => clearInterval(id);
  }, [reload, preview]);
  return [value, setValue, reload] as const;
}
