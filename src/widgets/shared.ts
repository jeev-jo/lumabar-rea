import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type WidgetSize = "s" | "m" | "l";

export type WidgetInstance = {
  id: string;
  kind: string;
  size: WidgetSize;
  config: Record<string, unknown>;
};

/** Props every tile and panel gets. `preview` = shown in the picker: no side effects. */
export type WidgetProps<C = Record<string, unknown>> = {
  instance: WidgetInstance & { config: C };
  preview?: boolean;
};

export const updateConfig = (id: string, config: unknown) => invoke("widget_update", { id, config });

/**
 * A widget's own stored data (todos, notes...), shared live between the dock tile
 * and its expanded view. Writes are saved by Rust and echoed to every window.
 */
export function useWidgetData<T>(id: string, initial: T, preview = false) {
  const [data, setData] = useState<T>(initial);
  const [loaded, setLoaded] = useState(preview);
  useEffect(() => {
    if (preview) return;
    let live = true;
    invoke<T | null>("widget_data_get", { id }).then((d) => {
      if (!live) return;
      if (d !== null && d !== undefined) setData(d);
      setLoaded(true);
    });
    const un = listen<{ id: string; data: T }>("widget-data-changed", (e) => {
      if (e.payload.id === id) setData(e.payload.data);
    });
    return () => {
      live = false;
      un.then((f) => f());
    };
  }, [id, preview]);
  const save = useCallback(
    (next: T) => {
      setData(next);
      if (!preview) invoke("widget_data_set", { id, data: next }).catch(console.error);
    },
    [id, preview],
  );
  return [data, save, loaded] as const;
}

export type Sample = {
  t: number;
  cpu: number;
  ram: number;
  ram_used_gb: number;
  ram_total_gb: number;
  down: number;
  up: number;
};

/** The last 60 one-second system samples, live. */
export function useStats(enabled = true) {
  const [samples, setSamples] = useState<Sample[]>([]);
  useEffect(() => {
    if (!enabled) return;
    invoke<Sample[]>("stats_history").then(setSamples).catch(console.error);
    const un = listen<Sample>("stats", (e) => setSamples((s) => [...s.slice(-59), e.payload]));
    return () => {
      un.then((f) => f());
    };
  }, [enabled]);
  return samples;
}

/** Re-render every `ms` milliseconds. */
export function useTick(ms: number) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), ms);
    return () => clearInterval(id);
  }, [ms]);
  return now;
}

/** Debounce a callback (for autosaving text as you type). */
export function useDebounced<A extends unknown[]>(fn: (...a: A) => void, ms: number) {
  const t = useRef<number | undefined>(undefined);
  const f = useRef(fn);
  f.current = fn;
  useEffect(() => () => window.clearTimeout(t.current), []);
  return useCallback(
    (...a: A) => {
      window.clearTimeout(t.current);
      t.current = window.setTimeout(() => f.current(...a), ms);
    },
    [ms],
  );
}

export function formatRate(bytesPerSec: number) {
  const units = ["B/s", "KB/s", "MB/s", "GB/s"];
  let v = bytesPerSec;
  let i = 0;
  while (v >= 1000 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v < 10 && i > 0 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}

export const uid = () => Math.random().toString(36).slice(2, 10);
