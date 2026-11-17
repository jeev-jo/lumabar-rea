import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

type State =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "latest" }
  | { kind: "available"; update: Update }
  | { kind: "downloading"; percent: number | null }
  | { kind: "error"; message: string };

/** Settings row: check GitHub Releases for a signed update and install it. */
export function UpdatesRow({ Row }: { Row: (p: { title: string; hint?: string; children: React.ReactNode }) => React.ReactElement }) {
  const [state, setState] = useState<State>({ kind: "idle" });

  const runCheck = async () => {
    setState({ kind: "checking" });
    try {
      const update = await check();
      setState(update ? { kind: "available", update } : { kind: "latest" });
    } catch (e) {
      setState({ kind: "error", message: String(e) });
    }
  };

  const install = async (update: Update) => {
    let total = 0;
    let got = 0;
    setState({ kind: "downloading", percent: null });
    try {
      await update.download((event) => {
        if (event.event === "Started") total = event.data.contentLength ?? 0;
        if (event.event === "Progress") {
          got += event.data.chunkLength;
          setState({ kind: "downloading", percent: total ? Math.round((got / total) * 100) : null });
        }
      });
      // The installer closes the dock; put the Windows taskbar back first.
      await invoke("prepare_update");
      await update.install();
      await relaunch();
    } catch (e) {
      setState({ kind: "error", message: String(e) });
    }
  };

  let hint = "Updates come from the project's GitHub releases and are checked against its signing key.";
  let button: React.ReactNode = (
    <button className="action" onClick={runCheck}>
      Check now
    </button>
  );
  switch (state.kind) {
    case "checking":
      hint = "Checking…";
      button = <button className="action" disabled>Check now</button>;
      break;
    case "latest":
      hint = "You have the latest version.";
      break;
    case "available":
      hint = `Version ${state.update.version} is available.`;
      button = (
        <button className="action primary" onClick={() => install(state.update)}>
          Install and restart
        </button>
      );
      break;
    case "downloading":
      hint = state.percent === null ? "Downloading…" : `Downloading… ${state.percent}%`;
      button = <button className="action" disabled>Installing</button>;
      break;
    case "error":
      hint = `Couldn't check for updates: ${state.message}`;
      break;
  }

  return (
    <Row title="Updates" hint={hint}>
      {button}
    </Row>
  );
}
