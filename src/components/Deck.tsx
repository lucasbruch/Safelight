import type { ReactNode } from "react";

/** The console's shared parts: lamps, keycaps, readouts and engraved labels. */

export type LampColor = "pick" | "reject" | "star" | "white" | "busy";

export function Lamp({ color = "white", lit = true, blink, title }: { color?: LampColor; lit?: boolean; blink?: boolean; title?: string }) {
  return <span className={`lamp ${color} ${lit ? "lit" : ""} ${blink ? "blink" : ""}`} title={title} aria-hidden={!title} />;
}

interface KeyProps {
  /** The keyboard key that does the same thing, printed on the cap. */
  legend?: string;
  label?: ReactNode;
  lamp?: LampColor;
  lit?: boolean;
  pressed?: boolean;
  wide?: boolean;
  onClick?: () => void;
  title?: string;
  disabled?: boolean;
  className?: string;
}

/** A keycap. Its lamp shows state; its legend is the shortcut. */
export function Key({ legend, label, lamp, lit, pressed, wide, onClick, title, disabled, className = "" }: KeyProps) {
  return (
    <button
      className={`key ${lamp ?? ""} ${lit ? "lit" : ""} ${pressed ? "down" : ""} ${wide ? "wide" : ""} ${className}`}
      onClick={onClick}
      title={title}
      disabled={disabled}
      aria-pressed={lamp ? !!lit : undefined}
    >
      {lamp && <Lamp color={lamp} lit={lit} />}
      {legend && <span className="legend">{legend}</span>}
      {label != null && <span className="key-label">{label}</span>}
    </button>
  );
}

/**
 * A labelled value. `slot` reserves room for the widest value in characters,
 * so digits tick over in place and never shift the rail around them.
 */
export function Readout({ label, children, lamp, lit = true, title, slot }: { label: string; children: ReactNode; lamp?: LampColor; lit?: boolean; title?: string; slot?: number }) {
  return (
    <div className="readout" title={title}>
      <span className="engraved">{label}</span>
      <span className="readout-value">
        {lamp && <Lamp color={lamp} lit={lit} />}
        <span className="slot" style={slot ? { minWidth: `${slot}ch` } : undefined}>{children}</span>
      </span>
    </div>
  );
}

/** A plate with a short engraved legend, for things that have no single-key shortcut (e.g. AI). */
export function Legend({ children }: { children: ReactNode }) {
  return <span className="legend">{children}</span>;
}

export function Engraved({ children, as: Tag = "span" }: { children: ReactNode; as?: "span" | "h2" | "h4" | "label" }) {
  return <Tag className="engraved">{children}</Tag>;
}
