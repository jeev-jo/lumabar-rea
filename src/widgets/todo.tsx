import { useState } from "react";
import { uid, useWidgetData, type WidgetProps } from "./shared";

type Item = { id: string; text: string; done: boolean };
type Data = { items: Item[] };

const DEMO: Data = {
  items: [
    { id: "1", text: "Reply to the design review", done: false },
    { id: "2", text: "Book train tickets", done: false },
    { id: "3", text: "Water the plants", done: true },
  ],
};

export function TodoTile({ instance, preview }: WidgetProps) {
  const [data] = useWidgetData<Data>(instance.id, preview ? DEMO : { items: [] }, preview);
  const open = data.items.filter((i) => !i.done);
  return (
    <div className={`w-todo ${instance.size}`}>
      <span className="check">✓</span>
      <b>{open.length}</b>
      {instance.size !== "s" && <span className="first">{open[0]?.text ?? "All done"}</span>}
    </div>
  );
}

export function TodoPanel({ instance }: WidgetProps) {
  const [data, save] = useWidgetData<Data>(instance.id, { items: [] });
  const [text, setText] = useState("");
  const add = () => {
    const t = text.trim();
    if (!t) return;
    save({ items: [...data.items, { id: uid(), text: t, done: false }] });
    setText("");
  };
  const open = data.items.filter((i) => !i.done);
  const done = data.items.filter((i) => i.done);
  const toggle = (id: string) => save({ items: data.items.map((i) => (i.id === id ? { ...i, done: !i.done } : i)) });
  const remove = (id: string) => save({ items: data.items.filter((i) => i.id !== id) });

  return (
    <div className="panel w-todo-panel">
      <h3>
        To-do <span className="now">{open.length} open</span>
      </h3>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          add();
        }}
      >
        <input autoFocus placeholder="Add a task and press Enter" value={text} onChange={(e) => setText(e.target.value)} />
      </form>
      <ul className="items">
        {[...open, ...done].map((i) => (
          <li key={i.id} className={i.done ? "done" : ""}>
            <label>
              <input type="checkbox" checked={i.done} onChange={() => toggle(i.id)} />
              <span>{i.text}</span>
            </label>
            <button className="remove" title="Delete" onClick={() => remove(i.id)}>
              ×
            </button>
          </li>
        ))}
      </ul>
      {done.length > 0 && (
        <button className="link" onClick={() => save({ items: open })}>
          Clear {done.length} completed
        </button>
      )}
    </div>
  );
}
