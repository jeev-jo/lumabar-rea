# Contributing to luma-ai-bar

Thanks for helping. Bug reports, widget ideas, fixes and new widgets are all welcome.

## Before you start

- **Bugs:** open an issue. Include your Windows version, the luma-ai-bar version (*Settings → About*), what you did, and what happened. A screenshot of the dock helps.
- **Ideas and bigger changes:** open an issue first, so we can agree on the approach before you write the code.
- **Small fixes** (typos, layout, an obvious bug) can go straight to a pull request.

The project doesn't have a license yet, so please ask in an issue before reusing its code elsewhere.

## Set up

You need Windows 10 or 11, Node 20+, Rust (stable, MSVC toolchain), and the Visual Studio C++ build tools.

Install the dependencies:

```bash
npm install
```

Start the dock in development:

```bash
npm run tauri dev
```

The dev dock replaces your taskbar just like the installed one. Quit it with **Ctrl+Alt+Shift+Q**; the Windows taskbar always comes back. Quit an installed copy first, so you don't end up with two docks.

Changes to files in `src/` reload instantly. Changes to Rust files rebuild and restart the dock.

### Handy switches for development

Set these before `npm run tauri dev`:

| Variable | What it does |
|---|---|
| `LUMA_DEBUG_SETTINGS=1` | Opens Settings at start |
| `LUMA_DEBUG_WIDGETS=1` | Opens the widget picker at start |
| `LUMA_DEBUG_POPOVER=<kind>` | Opens one widget's details view at start, e.g. `weather` |
| `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` | Lets you inspect the dock's pages from Edge or Chrome at `edge://inspect` |

Your settings live in `%LOCALAPPDATA%\luma_ai_bar\settings.json`. Back it up before experimenting.

## How the code is laid out

```
src/                  React front end (one page per window)
  App.tsx             The dock: apps, widgets, clock
  apps.tsx            App buttons, previews, badges
  SettingsPage.tsx    Settings window
  WidgetPicker.tsx    Widget picker
  liquid.tsx          Liquid glass shader
  theme.css           Colours for every theme and style
  widgets/
    registry.tsx      Every widget: name, group, sizes, tile and details view
    services.tsx      Widgets backed by an online service (revenue, analytics…)
    *.tsx             The widgets themselves
src-tauri/src/        Rust back end
  lib.rs              Start-up and the list of commands the front end can call
  apps.rs             Windows, pins, icons, badges
  taskbar.rs          Hiding the Windows taskbar and always bringing it back
  dock.rs             The dock window: size, position, edge
  settings.rs         Settings, Start with Windows
  online.rs           Online services (Stripe, GitHub, mail…)
  ai_usage.rs         AI plan limits and spend
  secrets.rs          Encrypting API keys with Windows DPAPI
site/                 The landing page (deployed by Vercel)
scripts/              Release helpers
```

## Adding a widget

1. Write a `Tile` (what sits on the dock) and a `Panel` (the details view) in a file under `src/widgets/`. Both get `WidgetProps`: the widget's `instance` (id, size, config) and `preview`, which is true in the picker. Show sample data when `preview` is true, so the picker card looks real.
2. Register it in `src/widgets/registry.tsx` with `w(kind, name, group, description, sizes, defaultSize, [panelWidth, panelHeight], { Tile, Panel })`.
3. If it needs data from Windows or the network, add a `#[tauri::command]` in Rust and list it in `invoke_handler` in `src-tauri/src/lib.rs`.
4. For an online service with an API key, add the provider to `PROVIDERS` and `fetch` in `src-tauri/src/online.rs`, and a `ServiceDef` to `DEFS` in `src/widgets/services.tsx`. Key storage, refreshing and the key form come for free.
5. Add styles to `src/widgets/widgets.css`. Use the colour variables from `theme.css`, so it works in light, dark and every style.
6. Check it at all three sizes, in light and dark, and in the picker.

## Rules that keep the dock trustworthy

- **The taskbar must always come back.** Anything that hides or changes the Windows taskbar goes through `taskbar.rs`, which restores it on quit, on crash and from the watchdog. Never leave a path where the dock can exit with the taskbar hidden.
- **Keys stay private.** Store API keys, tokens and passwords with `secrets.rs`, send them only to the service they belong to, and never log or display them.
- **Data stays on the PC.** Don't add analytics, telemetry or calls to services the user didn't set up.
- **Read, don't write, other apps' files.** For example, Claude Code's sign-in is only read.
- **Our own work only.** Don't copy code, images, icons or fonts from other dock apps or products. Use your own, or ones whose license allows it, and say where they came from.
- **Ask before acting.** Widgets must not change system settings, send messages or spend money unless the user clicks a control that says so.

## Check your change

Front-end types:

```bash
npx tsc --noEmit -p .
```

Rust tests (from `src-tauri`):

```bash
cargo test --lib
```

Some tests are marked `#[ignore]` because they read this PC's live data or call real services. Run them on purpose with `cargo test --lib -- --ignored`.

Then try the change in the running dock: the tile, its details view, the picker card, and quitting with Ctrl+Alt+Shift+Q.

## Pull requests

- Keep each pull request to one change, with a short description of what changed and how you tested it.
- Include a screenshot for anything visible.
- Write comments and UI text in plain language: say what something does, not how clever it is.
- Don't bump the version or edit release files; releases are made by the maintainer (see *Releasing an update* in the README).
