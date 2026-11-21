import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { type Patch, type UpdateMode, updateSettings, useSettings } from "./settings";
import { useManualUpdate } from "./Updates";
import "./settings-page.css";

type Display = { id: string; label: string; primary: boolean };
type TaskbarStatus = { applied: "complete" | "auto-hide" | "none"; complete_failed: boolean };
type PageId = "appearance" | "taskbar" | "widgets" | "apps" | "updates" | "about";

// ---- Controls ---------------------------------------------------------------------

function Segmented<T extends string>({ value, options, onChange }: { value: T; options: [T, string][]; onChange: (v: T) => void }) {
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
    <label className="toggle-wrap">
      <span className="state">{checked ? "On" : "Off"}</span>
      <button role="switch" aria-checked={checked} aria-label={label} className={`toggle${checked ? " on" : ""}`} onClick={() => onChange(!checked)}>
        <span />
      </button>
    </label>
  );
}

function Row({ icon, title, hint, children }: { icon?: React.ReactNode; title: string; hint?: React.ReactNode; children?: React.ReactNode }) {
  return (
    <div className="row">
      {icon && <span className="row-icon">{icon}</span>}
      <div className="text">
        <div className="title">{title}</div>
        {hint && <div className="hint">{hint}</div>}
      </div>
      {children && <div className="control">{children}</div>}
    </div>
  );
}

function Group({ title, children }: { title?: string; children: React.ReactNode }) {
  return (
    <section className="group">
      {title && <h3>{title}</h3>}
      <div className="set-card">{children}</div>
    </section>
  );
}

// ---- Icons (24px line icons) ---------------------------------------------------------

const I = (d: string) => (
  <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d={d} />
  </svg>
);
const ICONS = {
  appearance: I("M12 3a9 9 0 1 0 0 18c1.1 0 1.6-.9 1.2-1.8l-.4-.9c-.5-1 .2-2.3 1.4-2.3H17a4 4 0 0 0 4-4c0-5-4-9-9-9ZM7.5 11h.01M10 7.5h.01M15 7.5h.01"),
  taskbar: I("M3 6.5A2.5 2.5 0 0 1 5.5 4h13A2.5 2.5 0 0 1 21 6.5v11a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 17.5ZM3 16h18M8 18h.01M11 18h2"),
  widgets: I("M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z"),
  apps: I("M12 17v4M8 21h8M5 3h14l-2 7H7Z M9 10l-1 7h8l-1-7"),
  updates: I("M20 12a8 8 0 1 1-2.3-5.6M20 4v5h-5"),
  about: I("M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18ZM12 11v5M12 8h.01"),
  position: I("M4 5h16M4 19h16M8 9h8v6H8z"),
  display: I("M3 5h18v11H3zM9 20h6M12 16v4"),
  size: I("M4 14h6v6H4zM14 4h6v6h-6zM10 14l4-4"),
  style: I("M12 3l2.5 5.5L20 11l-5.5 2.5L12 19l-2.5-5.5L4 11l5.5-2.5z"),
  theme: I("M12 3a9 9 0 1 0 9 9 7 7 0 0 1-9-9Z"),
  hide: I("M3 3l18 18M10.6 6.1A9.9 9.9 0 0 1 12 6c5 0 9 6 9 6a17 17 0 0 1-2.6 3.2M6.6 6.6A17 17 0 0 0 3 12s4 6 9 6a9 9 0 0 0 3.4-.7"),
  desktop: I("M3 5h18v14H3zM17 5v14"),
  start: I("M5 12h14M13 6l6 6-6 6"),
  hover: I("M9 4v10l2.5-2.5L14 17l2-1-2.5-5H17Z"),
  grid: I("M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM15 13v6M12 16h6"),
  pins: I("M15 4l5 5-3 1-3 3 1 4-1 1-4-4-5 5M9 9l4-4"),
  bell: I("M6 16V11a6 6 0 0 1 12 0v5l2 2H4ZM10 21h4"),
  refresh: I("M20 12a8 8 0 1 1-2.3-5.6M20 4v5h-5"),
  keyboard: I("M3 7h18v10H3zM7 11h.01M11 11h.01M15 11h.01M8 14h8"),
  code: I("M9 8l-4 4 4 4M15 8l4 4-4 4"),
};

const PAGES: { id: PageId; label: string; blurb: string }[] = [
  { id: "appearance", label: "Appearance", blurb: "Where the dock sits and how it looks." },
  { id: "taskbar", label: "Taskbar", blurb: "How the dock works alongside Windows." },
  { id: "widgets", label: "Widgets", blurb: "What sits beside your apps." },
  { id: "apps", label: "Pinned apps", blurb: "The apps that are always on the dock." },
  { id: "updates", label: "Updates", blurb: "New versions, checked against the project's signing key." },
  { id: "about", label: "About", blurb: "Version, shortcuts and links." },
];

// ---- Page ----------------------------------------------------------------------------

export default function SettingsPage() {
  const settings = useSettings();
  const [page, setPage] = useState<PageId>(() => {
    try {
      return (localStorage.getItem("settings-page") as PageId) || "appearance";
    } catch {
      return "appearance";
    }
  });
  const [displays, setDisplays] = useState<Display[]>([]);
  const [autostart, setAutostart] = useState(false);
  const [version, setVersion] = useState("");
  const [pinsReset, setPinsReset] = useState(false);
  const [taskbar, setTaskbar] = useState<TaskbarStatus | null>(null);
  const updates = useManualUpdate();

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

  const go = (id: PageId) => {
    setPage(id);
    try {
      localStorage.setItem("settings-page", id);
    } catch {
      /* remembering the page is only a convenience */
    }
  };

  if (!settings) return null;
  const set = (patch: Patch) => updateSettings(patch).catch(console.error);
  const currentDisplay = settings.display ?? displays.find((d) => d.primary)?.id ?? "";
  const current = PAGES.find((p) => p.id === page) ?? PAGES[0];

  let updateHint = "";
  let updateButton: React.ReactNode = (
    <button className="action" onClick={updates.runCheck}>
      Check now
    </button>
  );
  const us = updates.state;
  switch (us.kind) {
    case "idle":
      updateHint = `You have version ${version}.`;
      break;
    case "checking":
      updateHint = "Checking…";
      updateButton = <button className="action" disabled>Checking…</button>;
      break;
    case "latest":
      updateHint = `You're up to date (version ${version}).`;
      break;
    case "available":
      updateHint = `Version ${us.update.version} is available. The dock restarts to install it.`;
      updateButton = (
        <button className="action primary" onClick={() => updates.install(us.update)}>
          Install and restart
        </button>
      );
      break;
    case "downloading":
      updateHint = us.percent === null ? "Downloading…" : `Downloading… ${us.percent}%`;
      updateButton = <button className="action" disabled>Installing…</button>;
      break;
    case "error":
      updateHint = `Couldn't check for updates: ${us.message}`;
      break;
  }

  return (
    <div className="settings-shell">
      <nav className="sidebar" aria-label="Settings sections">
        <div className="brand">
          <span className="logo" aria-hidden="true">
            <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
              <rect x="3" y="14" width="18" height="6" rx="3" />
              <path d="M7 10h.01M12 10h.01M17 10h.01" />
            </svg>
          </span>
          <div>
            <div className="name">luma-ai-bar</div>
            {version && <div className="version">Version {version}</div>}
          </div>
        </div>
        {PAGES.map((p) => (
          <button key={p.id} className={`nav${p.id === page ? " on" : ""}`} aria-current={p.id === page ? "page" : undefined} onClick={() => go(p.id)}>
            {ICONS[p.id]}
            {p.label}
          </button>
        ))}
      </nav>

      <main>
        <header>
          <h1>{current.label}</h1>
          <p>{current.blurb}</p>
        </header>

        {page === "appearance" && (
          <>
            <Group title="Layout">
              <Row icon={ICONS.position} title="Position" hint="You can also drag the dock to the other edge.">
                <Segmented value={settings.edge} options={[["bottom", "Bottom"], ["top", "Top"]]} onChange={(edge) => set({ edge })} />
              </Row>
              <Row icon={ICONS.display} title="Display" hint={displays.length < 2 ? "Only one display is connected." : "The screen the dock appears on."}>
                <select value={currentDisplay} onChange={(e) => set({ display: e.target.value })} disabled={displays.length < 2}>
                  {displays.map((d) => (
                    <option key={d.id} value={d.id}>
                      {d.label}
                    </option>
                  ))}
                </select>
              </Row>
              <Row icon={ICONS.size} title="Size" hint="Icon and widget size. The dock also shrinks when there's too much for the screen.">
                <Segmented value={settings.size} options={[["small", "Small"], ["medium", "Medium"], ["large", "Large"]]} onChange={(size) => set({ size })} />
              </Row>
            </Group>
            <Group title="Look">
              <Row
                icon={ICONS.style}
                title="Material"
                hint={
                  settings.style === "liquid"
                    ? "Liquid glass bends what moves behind the dock. While it's on, the dock doesn't appear in screenshots or screen sharing."
                    : settings.style === "glass"
                      ? "Glass lets the desktop show through, blurred."
                      : "A plain, opaque bar."
                }
              >
                <Segmented value={settings.style} options={[["liquid", "Liquid glass"], ["glass", "Glass"], ["solid", "Solid"]]} onChange={(style) => set({ style })} />
              </Row>
              <Row icon={ICONS.theme} title="Theme" hint="System follows Windows' light or dark mode.">
                <Segmented value={settings.theme} options={[["system", "System"], ["dark", "Dark"], ["light", "Light"]]} onChange={(theme) => set({ theme })} />
              </Row>
            </Group>
          </>
        )}

        {page === "taskbar" && (
          <>
            <Group title="Windows taskbar">
              <Row
                icon={ICONS.hide}
                title="When the dock is running"
                hint={
                  taskbar?.complete_failed
                    ? "Windows wouldn't stay hidden completely, so auto-hide is being used instead."
                    : settings.taskbar_mode === "complete"
                      ? "Stays hidden, even for Start and Search. It comes back when the dock quits."
                      : settings.taskbar_mode === "hide"
                        ? "Auto-hide still slides the taskbar in for Start and Search. It comes back when the dock quits."
                        : "The Windows taskbar stays where it is."
                }
              >
                <Segmented
                  value={settings.taskbar_mode}
                  options={[["hide", "Auto-hide"], ["complete", "Hide completely"], ["leave", "Don't hide"]]}
                  onChange={(taskbar_mode) => set({ taskbar_mode })}
                />
              </Row>
              <Row icon={ICONS.desktop} title="Show desktop button" hint="A thin button at the far end of the dock, like the Windows taskbar's.">
                <Toggle label="Show desktop button" checked={settings.peek_button !== false} onChange={(peek_button) => set({ peek_button })} />
              </Row>
            </Group>
            <Group title="Startup">
              <Row icon={ICONS.start} title="Start with Windows" hint="Opens the dock when you sign in.">
                <Toggle
                  label="Start with Windows"
                  checked={autostart}
                  onChange={(on) => invoke<boolean>("set_autostart", { enabled: on }).then(setAutostart).catch(console.error)}
                />
              </Row>
            </Group>
          </>
        )}

        {page === "widgets" && (
          <>
            <Group>
              <Row icon={ICONS.grid} title="Add, remove and arrange widgets" hint={`${settings.widgets.length} widget${settings.widgets.length === 1 ? "" : "s"} in the dock. Drag them on the dock to move them.`}>
                <button className="action primary" onClick={() => invoke("open_widget_picker")}>
                  Open widgets
                </button>
              </Row>
              <Row icon={ICONS.hover} title="Show details on hover" hint="Rest the pointer on a widget for a quick look. Click a widget to keep its details open.">
                <Toggle label="Show details on hover" checked={settings.glance} onChange={(glance) => set({ glance })} />
              </Row>
            </Group>
            <p className="footnote">API keys you give widgets are encrypted with Windows' own protection and only sent to the service they belong to.</p>
          </>
        )}

        {page === "apps" && (
          <Group>
            <Row icon={ICONS.pins} title="Re-import pins" hint="Replaces the dock's pins with the apps pinned to the Windows taskbar. Right-click an app on the dock to pin or unpin it.">
              <button className="action" onClick={() => invoke("reset_pins").then(() => setPinsReset(true)).catch(console.error)}>
                {pinsReset ? "Done" : "Re-import"}
              </button>
            </Row>
          </Group>
        )}

        {page === "updates" && (
          <>
            <Group>
              <Row icon={ICONS.refresh} title="luma-ai-bar" hint={updateHint}>
                {updateButton}
              </Row>
              <Row
                icon={ICONS.bell}
                title="New versions"
                hint={
                  settings.updates === "auto"
                    ? "Checked when the dock starts and every six hours, and installed right away. The dock restarts briefly."
                    : settings.updates === "off"
                      ? "Never checked automatically. Use Check now."
                      : "Checked when the dock starts and every six hours. An Update button appears on the dock when one is ready."
                }
              >
                <Segmented<UpdateMode>
                  value={settings.updates ?? "notify"}
                  options={[["notify", "Tell me"], ["auto", "Install"], ["off", "Off"]]}
                  onChange={(updates) => set({ updates })}
                />
              </Row>
            </Group>
            <p className="footnote">Updates come from the project's GitHub releases and are only installed if their signature matches.</p>
          </>
        )}

        {page === "about" && (
          <>
            <Group>
              <Row icon={ICONS.about} title="luma-ai-bar" hint={`Version ${version}. A dock for Windows 11 with widgets for your PC, media, clocks and AI usage.`} />
              <Row icon={ICONS.keyboard} title="Quit from anywhere" hint="Puts the Windows taskbar back, even if the dock is stuck.">
                <span className="keys">
                  <kbd>Ctrl</kbd>
                  <kbd>Alt</kbd>
                  <kbd>Shift</kbd>
                  <kbd>Q</kbd>
                </span>
              </Row>
              <Row icon={ICONS.code} title="Source and releases" hint="github.com/jeev-jo/lumabar-rea">
                <button className="action" onClick={() => invoke("open_link", { url: "https://github.com/jeev-jo/lumabar-rea" })}>
                  Open
                </button>
              </Row>
            </Group>
          </>
        )}
      </main>
    </div>
  );
}
