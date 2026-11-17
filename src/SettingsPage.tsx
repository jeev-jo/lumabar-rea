import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { type Patch, updateSettings, useSettings } from "./settings";
import "./settings-page.css";

type Display = { id: string; label: string; primary: boolean };

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
          title="Hide the Windows taskbar"
          hint="Auto-hides it while the dock runs and puts it back when the dock quits. Ctrl+Alt+Shift+Q quits the dock from anywhere."
        >
          <Toggle
            label="Hide the Windows taskbar"
            checked={settings.taskbar_mode === "hide"}
            onChange={(on) => set({ taskbar_mode: on ? "hide" : "leave" })}
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
