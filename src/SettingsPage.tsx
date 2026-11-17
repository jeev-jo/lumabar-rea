import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { type Patch, updateSettings, useSettings } from "./settings";
import "./settings-page.css";

type Display = { id: string; label: string; primary: boolean };
type TaskbarStatus = { applied: "complete" | "auto-hide" | "none"; complete_failed: boolean };

function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: [T, string][];
  onChange: (v: T) => void;
}) {
  return (
    <div className="segmented" role="radiogroup">
      {options.map(([v, label]) => (
        <button key={v} role="radio" aria-checked={v === value} className={v === value ? "on" : ""} onClick={() => onChange(v)}>
          {label}
        </button>
      ))}
    </div>
  );
}

function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button role="switch" aria-checked={checked} aria-label={label} className={`toggle${checked ? " on" : ""}`} onClick={() => onChange(!checked)}>
      <span />
    </button>
  );
}

function Row({ title, hint, children }: { title: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="row">
      <div className="text">
        <div className="title">{title}</div>
        {hint && <div className="hint">{hint}</div>}
      </div>
      <div className="control">{children}</div>
    </div>
  );
}

export default function SettingsPage() {
  const settings = useSettings();
  const [displays, setDisplays] = useState<Display[]>([]);
  const [autostart, setAutostart] = useState(false);
  const [version, setVersion] = useState("");
  const [pinsReset, setPinsReset] = useState(false);
  const [taskbar, setTaskbar] = useState<TaskbarStatus | null>(null);

  useEffect(() => {
    document.documentElement.classList.add("settings-page");
    invoke<Display[]>("list_displays").then(setDisplays).catch(console.error);
    invoke<boolean>("get_autostart").then(setAutostart).catch(console.error);
    getVersion().then(setVersion).catch(() => {});
    // Displays can be plugged in while the window is open.
    const refresh = () => invoke<Display[]>("list_displays").then(setDisplays).catch(console.error);
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, []);

  // Ask what actually happened, since "hide completely" can fall back to auto-hide.
  useEffect(() => {
    invoke<TaskbarStatus>("taskbar_status").then(setTaskbar).catch(console.error);
  }, [settings?.taskbar_mode]);

  if (!settings) return null;
  const set = (patch: Patch) => updateSettings(patch).catch(console.error);
  const currentDisplay = settings.display ?? displays.find((d) => d.primary)?.id ?? "";

  return (
    <main>
      <header>
        <h1>luma-ai-bar</h1>
        {version && <span className="version">v{version}</span>}
      </header>

      <section>
        <h2>Dock</h2>
        <Row title="Position">
          <Segmented value={settings.edge} options={[["bottom", "Bottom"], ["top", "Top"]]} onChange={(edge) => set({ edge })} />
        </Row>
        <Row title="Display" hint={displays.length < 2 ? "Only one display is connected." : undefined}>
          <select value={currentDisplay} onChange={(e) => set({ display: e.target.value })} disabled={displays.length < 2}>
            {displays.map((d) => (
              <option key={d.id} value={d.id}>
                {d.label}
              </option>
            ))}
          </select>
        </Row>
        <Row title="Size">
          <Segmented
            value={settings.size}
            options={[["small", "Small"], ["medium", "Medium"], ["large", "Large"]]}
            onChange={(size) => set({ size })}
          />
        </Row>
        <Row
          title="Style"
          hint={
            settings.style === "liquid"
              ? "Liquid glass bends what moves behind the dock. While it's on, the dock doesn't appear in screenshots or screen sharing."
              : "Glass lets the desktop show through, blurred."
          }
        >
          <Segmented
            value={settings.style}
            options={[
              ["liquid", "Liquid glass"],
              ["glass", "Glass"],
              ["solid", "Solid"],
            ]}
            onChange={(style) => set({ style })}
          />
        </Row>
        <Row title="Theme" hint="System follows Windows' light or dark mode.">
          <Segmented
            value={settings.theme}
            options={[["system", "System"], ["dark", "Dark"], ["light", "Light"]]}
            onChange={(theme) => set({ theme })}
          />
        </Row>
      </section>

      <section>
        <h2>Windows</h2>
        <Row
          title="Windows taskbar"
          hint={
            taskbar?.complete_failed
              ? "Windows wouldn't stay hidden completely, so auto-hide is being used instead."
              : settings.taskbar_mode === "complete"
                ? "Stays hidden even for Start and Search. Comes back when the dock quits; Ctrl+Alt+Shift+Q quits it from anywhere."
                : "Auto-hide still slides the taskbar in for Start and Search. It comes back when the dock quits; Ctrl+Alt+Shift+Q quits it from anywhere."
          }
        >
          <Segmented
            value={settings.taskbar_mode}
            options={[
              ["hide", "Auto-hide"],
              ["complete", "Hide completely"],
              ["leave", "Don't hide"],
            ]}
            onChange={(taskbar_mode) => set({ taskbar_mode })}
          />
        </Row>
        <Row title="Start with Windows" hint="Opens the dock when you sign in.">
          <Toggle
            label="Start with Windows"
            checked={autostart}
            onChange={(on) => invoke<boolean>("set_autostart", { enabled: on }).then(setAutostart).catch(console.error)}
          />
        </Row>
      </section>

      <section>
        <h2>Widgets</h2>
        <Row title="Show details on hover" hint="Rest the pointer on a widget to see its details. Click a widget to keep them open.">
          <Toggle label="Show details on hover" checked={settings.glance} onChange={(glance) => set({ glance })} />
        </Row>
        <Row title="Add, remove and arrange widgets" hint={`${settings.widgets.length} in the dock`}>
          <button className="action" onClick={() => invoke("open_widget_picker")}>
            Open widgets
          </button>
        </Row>
      </section>

      <section>
        <h2>Pinned apps</h2>
        <Row title="Re-import pins" hint="Replaces the dock's pins with the apps pinned to the Windows taskbar.">
          <button
            className="action"
            onClick={() => {
              invoke("reset_pins").then(() => setPinsReset(true)).catch(console.error);
            }}
          >
            {pinsReset ? "Done" : "Re-import"}
          </button>
        </Row>
      </section>
    </main>
  );
}
