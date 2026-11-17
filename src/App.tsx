import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type Edge = "top" | "bottom";

function useClock() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(id);
  }, []);
  return now;
}

export default function App() {
  const [edge, setEdge] = useState<Edge>("bottom");
  const [dragging, setDragging] = useState(false);
  const dragStartY = useRef<number | null>(null);
  const now = useClock();

  useEffect(() => {
    invoke<Edge>("dock_get_edge").then(setEdge).catch(console.error);
  }, []);

  // Drag the bar towards the other screen edge and let go: it snaps there.
  function onPointerDown(e: React.PointerEvent<HTMLDivElement>) {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    dragStartY.current = e.screenY;
    e.currentTarget.setPointerCapture(e.pointerId);
  }
  function onPointerMove(e: React.PointerEvent<HTMLDivElement>) {
    if (dragStartY.current === null) return;
    if (Math.abs(e.screenY - dragStartY.current) > 24) setDragging(true);
  }
  function onPointerUp(e: React.PointerEvent<HTMLDivElement>) {
    if (dragStartY.current === null) return;
    const moved = Math.abs(e.screenY - dragStartY.current) > 24;
    dragStartY.current = null;
    setDragging(false);
    if (!moved) return;
    const target: Edge = e.screenY < window.screen.height / 2 ? "top" : "bottom";
    if (target !== edge) {
      setEdge(target);
      invoke("dock_set_edge", { edge: target }).catch(console.error);
    }
  }

  return (
    <div className={`strip edge-${edge}`}>
      <div
        className={`bar${dragging ? " dragging" : ""}`}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
      >
        <span className="brand">luma</span>
        <span className="spacer" />
        <span className="clock">
          {now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
          <small>{now.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" })}</small>
        </span>
        <button className="quit" title="Quit luma-ai-bar" onClick={() => invoke("quit_app")}>
          ×
        </button>
      </div>
    </div>
  );
}
