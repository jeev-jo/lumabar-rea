import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import type { UpdateMode } from "./settings";

/** Download a signed update, put the Windows taskbar back, install and restart. */
export async function installUpdate(update: Update, onProgress?: (percent: number | null) => void) {
  let total = 0;
  let got = 0;
  onProgress?.(null);
  await update.download((event) => {
    if (event.event === "Started") total = event.data.contentLength ?? 0;
    if (event.event === "Progress") {
      got += event.data.chunkLength;
      onProgress?.(total ? Math.round((got / total) * 100) : null);
    }
  });
  // The installer closes the dock; put the Windows taskbar back first.
  await invoke("prepare_update");
  await update.install();
  await relaunch();
}

const FIRST_CHECK = 60_000;
const EVERY = 6 * 60 * 60_000;

/**
 * The dock checks for a new version a minute after it starts and every six
 * hours after that. With "auto" it installs it straight away; with "notify"
 * it hands it back so the dock can offer it.
 */
export function useUpdateCheck(mode: UpdateMode | undefined) {
  const [update, setUpdate] = useState<Update | null>(null);
  const [percent, setPercent] = useState<number | null | undefined>(undefined);
  const busy = useRef(false);

  const install = async (u: Update) => {
    if (busy.current) return;
    busy.current = true;
    try {
      await installUpdate(u, setPercent);
    } catch (e) {
      console.error("update failed", e);
      setPercent(undefined);
    } finally {
      busy.current = false;
    }
  };

  useEffect(() => {
    if (!mode || mode === "off") {
      setUpdate(null);
      return;
    }
    const run = () =>
      check()
        .then((u) => {
          setUpdate(u);
          if (u && mode === "auto") install(u);
        })
        .catch(() => {});
    const first = setTimeout(run, FIRST_CHECK);
    const every = setInterval(run, EVERY);
    return () => {
      clearTimeout(first);
      clearInterval(every);
    };
  }, [mode]);

  return { update, installing: percent !== undefined, percent, install: () => update && install(update) };
}

type State =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "latest" }
  | { kind: "available"; update: Update }
  | { kind: "downloading"; percent: number | null }
  | { kind: "error"; message: string };

/** Settings: "Check now", and installing what it finds. */
export function useManualUpdate() {
  const [state, setState] = useState<State>({ kind: "idle" });
  const runCheck = async () => {
    setState({ kind: "checking" });
    try {
      const u = await check();
      setState(u ? { kind: "available", update: u } : { kind: "latest" });
    } catch (e) {
      setState({ kind: "error", message: String(e) });
    }
  };
  const install = async (u: Update) => {
    try {
      await installUpdate(u, (percent) => setState({ kind: "downloading", percent }));
    } catch (e) {
      setState({ kind: "error", message: String(e) });
    }
  };
  return { state, runCheck, install };
}
