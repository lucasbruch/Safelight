import { useEffect } from "react";
import { Icon } from "./Icon";

export type ToastMsg = { text: string; bad?: boolean; ms?: number };

export function Toast({ msg, onDone }: { msg: ToastMsg; onDone: () => void }) {
  useEffect(() => {
    const t = setTimeout(onDone, msg.ms ?? (msg.bad ? 8000 : 4500));
    return () => clearTimeout(t);
  }, [msg, onDone]);
  return (
    <div className={`toast ${msg.bad ? "bad" : ""}`} role="status">
      <Icon name={msg.bad ? "info" : "check"} />
      <span>{msg.text}</span>
      <button className="btn ghost small icon-btn" onClick={onDone} aria-label="Dismiss">
        <Icon name="x" size={14} />
      </button>
    </div>
  );
}
