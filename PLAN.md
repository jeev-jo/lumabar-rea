# luma_ai_bar: full plan

A dock for Windows 11 that takes over the taskbar's job, built from scratch with our own code, name and artwork. lumabar is the reference for *what* it does; none of its code, shaders, images or fonts are used.

## 1. Will it be the same as the Windows taskbar?

Mostly yes, with three known gaps. "Taskbar parity" breaks down like this:

| Taskbar job | Can we match it? | How |
|---|---|---|
| Show pinned apps | Yes | Read the shortcuts in `%AppData%\Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar` |
| Show open windows, grouped per app | Yes | `EnumWindows` with the Alt-Tab visibility rules, grouped by AppUserModelID; `SetWinEventHook` for live updates |
| Click to focus / minimize / launch | Yes | `SetForegroundWindow`, `ShowWindow`, `ShellExecuteW` |
| Hover previews of windows | Yes | DWM live thumbnails (`DwmRegisterThumbnail`) in a small native popup window |
| Flashing "needs attention" | Yes | Shell hook window (`RegisterShellHookWindow`, `HSHELL_FLASH`) |
| Reserve screen space so maximized windows don't cover it | Yes | Register as an AppBar (`SHAppBarMessage` `ABM_NEW` / `ABM_SETPOS`) |
| Get out of the way for fullscreen games and video | Yes | AppBar `ABN_FULLSCREENAPP` notification |
| Hide the real taskbar while we run, restore on exit | Yes | `ABM_SETSTATE` auto-hide, with crash-safe restore (section 5) |
| Start button, clock, volume, network, battery | Yes | Send the Win key; standard Windows APIs for the rest |
| Multi-monitor and per-monitor DPI | Yes | One dock window per chosen display |
| **Pinned Store (UWP) apps** | Partly | They are not in the shortcuts folder; needs a second lookup via `shell:AppsFolder`. Stretch. |
| **Jump lists** (right-click recent files) | Partly | Readable from `AutomaticDestinations` files, but the format is undocumented. Stretch. |
| **System tray icons** | Hardest | Windows has no official API to host other apps' tray icons. Options in section 6. |
| **Win+1…9 shortcuts** | Partly | Explorer owns these keys; needs a low-level keyboard hook. Stretch. |
| Notification centre / quick settings flyouts | No | We open the Windows ones (`Win+N`, `Win+A`) instead of rebuilding them |

## 2. Stack

- **Shell:** Tauri 2 (Rust backend, WebView2 UI). WebView2 157 is already on this PC.
- **UI:** React + TypeScript + Vite, zustand for state. Font: Segoe UI Variable (ships with Windows 11).
- **Windows APIs:** the `windows` crate. No admin rights, no Explorer patching, no DLL injection.
- **Storage:** JSON settings file in `%AppData%\luma_ai_bar`; SQLite later for history-type widgets.

**Missing on this PC:** Rust (`rustc`/`cargo` not found). Node 26, npm 11, Git and a Visual Studio install are present. Phase 0 installs Rust via rustup and confirms the MSVC C++ build tools.

## 3. Architecture

```
React UI (dock, widgets, settings)
   │  invoke(command) ▲ events
Rust backend
   ├─ dock      window placement, AppBar, edges, z-order, fullscreen
   ├─ taskbar   hide / restore the Windows taskbar, crash guard
   ├─ apps      pinned list, icons, launch
   ├─ windows   open-window tracking, focus, thumbnails, attention
   ├─ system    cpu, ram, network, battery, volume, brightness, media
   ├─ capture   screen region behind the dock (for glass)
   ├─ widgets   weather, calendar, AI usage, …
   └─ settings  load / save, autostart, updater
```

Rule: the UI never calls the network or Windows directly; everything goes through a named Rust command. That keeps secrets out of the web layer.

## 4. Phases

Each phase ends with something you can run and see. Sizes are relative: S, M, L.

### Phase 0 — Setup (S)
- Install Rust, verify C++ build tools, scaffold Tauri 2 + React + TS, `git init`.
- **Done when:** `npm run tauri dev` opens an empty window.

### Phase 1 — The dock window (M)
- Borderless, transparent, always-on-top, no taskbar entry, not in Alt-Tab.
- Sits on the bottom edge of the primary display at the right DPI; drag to snap to top.
- Registers as an AppBar so maximized windows stop at its edge.
- **Done when:** a rounded bar sits on the screen edge and a maximized window does not cover it.

### Phase 2 — Taskbar takeover (M)
- Set the Windows taskbar to auto-hide on start; restore the exact previous state on exit.
- Crash guard (section 5). Setting: auto-hide vs leave alone.
- Step aside for fullscreen apps.
- **Done when:** the taskbar is gone while we run, back after quit *and* after killing the process.

### Phase 3 — Apps and windows (L) ← core parity
- Pinned apps with real icons; open windows grouped under their app; running indicator dots.
- Click: launch / focus / minimize / cycle. Middle-click: new instance. Right-click menu: close, pin, unpin.
- Live updates when windows open, close, retitle or flash.
- Drag to reorder; drop a file on an app to open it with that app.
- **Done when:** you can work a full day without the real taskbar for switching apps.

### Phase 4 — Hover previews (M)
- Native popup with live DWM thumbnails per window, click to focus, × to close.
- **Done when:** hovering an app with 3 windows shows 3 live previews.

### Phase 5 — System strip (M)
- Start button, clock/date, volume (with scroll), network, battery, media now-playing with controls.
- Clicking them opens the Windows flyouts (quick settings, notification centre, calendar).
- **Done when:** the right side of the dock replaces the taskbar's clock and status area.

### Phase 6 — Settings and polish (M)
- Settings window: edge, size (3 sizes), theme (dark, light), which display(s), autostart.
- Tray icon for our own app (quit, settings). Smooth hover/magnify animation.
- **Done when:** everything in Phases 1–5 is configurable without editing files.

### Phase 7 — Widget framework + first widgets (L)
- A widget = a manifest (id, sizes, settings schema) + a React tile + optional Rust data command.
- Widget picker with drag-into-place. Popover window for expanded views.
- First set: CPU/RAM/network chart, weather (open-meteo, no key), todo, notes, pomodoro, world clock.
- **Done when:** you can add, remove and rearrange six widgets from a picker.

### Phase 8 — AI usage widgets (M)
- Default path: you paste an API key or token per service; stored with Windows DPAPI, never in the UI layer.
- Optional, off by default, clearly labelled experimental: read Claude Code's own sign-in file on this PC to show plan limits. It relies on an internal endpoint that can change or be blocked without notice.
- **Done when:** a Claude tile shows live usage and degrades gracefully ("last reading at …") on failure.

### Phase 9 — Glass theme (L)
- Rust captures the screen region behind the dock (DXGI Desktop Duplication, GDI fallback), excluding our own window (`WDA_EXCLUDEFROMCAPTURE`).
- Our own WebGL2 shader: blur, edge refraction, light rim. Only re-captures when something behind the dock changes.
- **Done when:** the glass theme holds 60 fps while dragging a window behind it and uses ~0% CPU when idle.

### Phase 10 — Ship (M)
- MSI installer, app icon, autostart entry, signed auto-update from GitHub Releases (Tauri updater with our own key).
- **Done when:** a clean Windows 11 machine installs it and updates from v0.1.0 to v0.1.1 by itself.

### Stretch (after v1)
Multi-monitor docks with per-display window filtering · Store-app pins · jump lists · Win+number keys · system tray hosting · screen time · calendar (ICS) · stocks · Gmail/GitHub tiles · Windows 10 support · 32-bit build.

## 5. Safety rules for touching the taskbar

Hiding someone's taskbar is the one thing here that can leave a PC in a bad state, so:
1. Before changing anything, write the taskbar's original state to disk.
2. Restore it on normal exit, on panic, and on Windows session end.
3. On every launch, if a leftover state file exists, restore first, then proceed.
4. Default to the documented auto-hide switch. "Hide completely" (hiding `Shell_TrayWnd` and every `Shell_SecondaryTrayWnd` with `ShowWindow`) is opt-in only: it is verified after every attempt, re-applied when Explorer re-shows or recreates the taskbar, and falls back to auto-hide if Windows won't keep it hidden. Restoring always shows the taskbar windows again. Never kill or patch Explorer.
5. A global "panic" hotkey that quits the dock and restores the taskbar.

## 6. Open decisions (my defaults in bold)

1. **Tray icons:** **v1 shows a button that pops the real Windows tray overflow**; hosting icons ourselves is a stretch goal and fragile across Windows updates.
2. **Default edge:** **bottom**, draggable to top.
3. **Windows versions:** **Windows 11 x64 only for v1**; Windows 10 and 32-bit later.
4. **Name / branding:** working name **luma_ai_bar**; needs its own icon and a name that isn't confusable with lumabar if you ever publish it.
5. **Scope of v1:** **Phases 0–6** is a usable taskbar replacement. Widgets, AI tiles and glass come after.

## 7. Risks

| Risk | Mitigation |
|---|---|
| Windows 11 Insider builds (this PC is on 26300) change taskbar internals | Stick to documented APIs; feature-detect; safe fallbacks |
| WebView2 transparent always-on-top windows flicker or steal focus | `WS_EX_NOACTIVATE` + `WS_EX_TOOLWINDOW`; test early in Phase 1 |
| Window list shows ghost or missing windows | Follow the documented Alt-Tab rules; check cloaked state via DWM |
| Glass capture cost | Dirty-region driven capture, downscaled, paused when idle or on battery |
| Internal AI endpoints break | API-key path is the default; experimental path isolated per provider |

## 8. How each phase gets verified

Run the dev build after every phase and check the "done when" line on this PC. I can launch it, take screenshots and read logs; anything that needs a human eye (animation feel, glass quality) I'll flag for you to look at.
