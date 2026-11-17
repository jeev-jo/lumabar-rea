import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useSettings } from "./settings";
import { byKind } from "./widgets/registry";
import "./widgets/widgets.css";

/** The widget popover: the expanded view of whichever widget was clicked. */
export default function Popover() {
  const settings = useSettings();
  const [id, setId] = useState<string | null>(null);

  useEffect(() => {
    document.documentElement.classList.add("popover-page");
    const un = listen<{ id: string }>("popover-open", (e) => setId(e.payload.id));
    const esc = (e: KeyboardEvent) => e.key === "Escape" && invoke("popover_hide");
    window.addEventListener("keydown", esc);
    return () => {
      un.then((f) => f());
      window.removeEventListener("keydown", esc);
    };
  }, []);

  const instance = settings?.widgets.find((w) => w.id === id);
  const kind = instance && byKind(instance.kind);
  if (!instance || !kind) return null;
  const Panel = kind.Panel;
  // Keyed by id so switching widgets starts the panel fresh.
  return (
    <div
      className="popover"
      onMouseEnter={() => invoke("popover_hover", { source: "popover", inside: true })}
      onMouseLeave={() => invoke("popover_hover", { source: "popover", inside: false })}
    >
      <Panel key={instance.id} instance={instance} />
    </div>
  );
}
