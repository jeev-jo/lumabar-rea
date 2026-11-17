import { useEffect, useState } from "react";
import { useDebounced, useWidgetData, type WidgetProps } from "./shared";

type Data = { text: string };

export function NotesTile({ instance, preview }: WidgetProps) {
  const [data] = useWidgetData<Data>(instance.id, { text: preview ? "Ideas for the weekend…" : "" }, preview);
  const first = data.text.split("\n").find((l) => l.trim()) ?? "";
  return (
    <div className={`w-notes ${instance.size}`}>
      <span className="glyph">{""}</span>
      {instance.size !== "s" && <span className="first">{first || "Empty note"}</span>}
    </div>
  );
}

export function NotesPanel({ instance }: WidgetProps) {
  const [data, save, loaded] = useWidgetData<Data>(instance.id, { text: "" });
  const [text, setText] = useState(data.text);
  const [saved, setSaved] = useState(true);
  // Take the stored text once it arrives (or changes elsewhere) while not typing.
  useEffect(() => {
    if (saved) setText(data.text);
  }, [data.text, loaded]);
  const persist = useDebounced((t: string) => {
    save({ text: t });
    setSaved(true);
  }, 400);
  return (
    <div className="panel w-notes-panel">
      <h3>
        Notes <span className="now">{saved ? "Saved" : "Saving…"}</span>
      </h3>
      <textarea
        autoFocus
        value={text}
        placeholder="Write anything. It's saved on this PC as you type."
        onChange={(e) => {
          setText(e.target.value);
          setSaved(false);
          persist(e.target.value);
        }}
      />
    </div>
  );
}
