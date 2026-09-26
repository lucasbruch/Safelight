import { useEffect, useRef, useState } from "react";
import { formatDate } from "../api";
import { Key, Lamp, Readout } from "../components/Deck";
import { activeExtraCount, AI_FILTERS, emptyFilters, FilterOptions, Filters, FlagFilter } from "./filters";

interface Props {
  filters: Filters;
  setFilters: (f: Filters) => void;
  options: FilterOptions;
  counts: Record<FlagFilter, number>;
  shown: number;
  thumbSize: number;
  setThumbSize: (n: number) => void;
  showThumbSize: boolean;
}

function toggle<T>(list: T[], v: T): T[] {
  return list.includes(v) ? list.filter((x) => x !== v) : [...list, v];
}

export default function FilterBar({ filters: f, setFilters, options, counts, shown, thumbSize, setThumbSize, showThumbSize }: Props) {
  const [open, setOpen] = useState(false);
  const [alignRight, setAlignRight] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const extra = activeExtraCount(f);
  const set = (patch: Partial<Filters>) => setFilters({ ...f, ...patch });

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => ref.current && !ref.current.contains(e.target as Node) && setOpen(false);
    const esc = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      setOpen(false);
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", esc, true);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", esc, true);
    };
  }, [open]);

  const toggleOpen = () => {
    // Open towards whichever side has room, so the panel never runs off the window.
    const r = ref.current?.getBoundingClientRect();
    setAlignRight(!!r && r.left + 380 > window.innerWidth - 16);
    setOpen((o) => !o);
  };

  const flagTabs: { key: FlagFilter; label: string }[] = [
    { key: "all", label: "All" },
    { key: "picks", label: "Picks" },
    { key: "unflagged", label: "Unflagged" },
    { key: "rejects", label: "Rejects" },
  ];

  return (
    <div className="filterbar">
      <span className="engraved group-label collapse-label">Flag</span>
      <div className="bank" role="radiogroup" aria-label="Flag">
        {flagTabs.map((t) => (
          <button key={t.key} role="radio" aria-checked={f.flag === t.key} className={f.flag === t.key ? "on" : ""} onClick={() => set({ flag: t.key })}>
            <Lamp color="white" lit={f.flag === t.key} />
            {t.label} <span className="num bank-count" style={{ minWidth: `${String(counts.all).length}ch` }}>{counts[t.key].toLocaleString()}</span>
          </button>
        ))}
      </div>

      <span className="engraved group-label collapse-label">Rating</span>
      <div className="bank rating-filter" role="radiogroup" aria-label="Minimum star rating">
        {[0, 1, 2, 3, 4, 5].map((r) => (
          <button key={r} role="radio" aria-checked={f.minRating === r} className={f.minRating === r ? "on" : ""} onClick={() => set({ minRating: r })}
            title={r === 0 ? "Any rating" : `${r} star${r > 1 ? "s" : ""} or more`}>
            <Lamp color={r === 0 ? "white" : "star"} lit={f.minRating === r} />
            {r === 0 ? "Any" : `${r}+`}
          </button>
        ))}
      </div>

      <span className="engraved group-label collapse-label">Filters</span>
      <div style={{ position: "relative" }} ref={ref}>
        <button className={`key small ${extra ? "lit" : ""} ${open ? "open" : ""}`} onClick={toggleOpen} aria-expanded={open}
          title={extra ? `${extra} more filter${extra > 1 ? "s" : ""} on` : "Camera, day, lens, AI flags, content, people"}>
          <Lamp color="white" lit={extra > 0} />
          <span className="key-label">More{extra ? <span className="count-badge">{extra}</span> : null}</span>
        </button>
        {open && (
          <div className={`popover ${alignRight ? "align-right" : ""}`} style={{ width: 380, maxHeight: "70vh", overflowY: "auto" }}>
            <Section title="AI helper">
              {AI_FILTERS.map((a) => (
                <button key={a.key} className={`chip ${f.ai.includes(a.key) ? "on" : ""}`} onClick={() => set({ ai: toggle(f.ai, a.key) })}>
                  {a.label}
                </button>
              ))}
            </Section>
            {options.cameras.length > 1 && (
              <Section title="Camera">
                {options.cameras.map((c) => (
                  <button key={c} className={`chip ${f.cameras.includes(c) ? "on" : ""}`} onClick={() => set({ cameras: toggle(f.cameras, c) })}>{c}</button>
                ))}
              </Section>
            )}
            {options.dates.length > 1 && (
              <Section title="Day">
                {options.dates.map((d) => (
                  <button key={d} className={`chip ${f.dates.includes(d) ? "on" : ""}`} onClick={() => set({ dates: toggle(f.dates, d) })}>
                    {d === "Unknown" ? d : formatDate(d)}
                  </button>
                ))}
              </Section>
            )}
            {options.hasVideo && (
              <Section title="Type">
                {(["all", "photo", "video"] as const).map((k) => (
                  <button key={k} className={`chip ${f.kind === k ? "on" : ""}`} onClick={() => set({ kind: k })}>
                    {k === "all" ? "Photos & videos" : k === "photo" ? "Photos" : "Videos"}
                  </button>
                ))}
              </Section>
            )}
            <Section title="Flash">
              {(["any", "yes", "no"] as const).map((k) => (
                <button key={k} className={`chip ${f.flash === k ? "on" : ""}`} onClick={() => set({ flash: k })}>
                  {k === "any" ? "Any" : k === "yes" ? "Flash fired" : "No flash"}
                </button>
              ))}
            </Section>
            {options.lenses.length > 1 && (
              <Section title="Lens">
                {options.lenses.map((l) => (
                  <button key={l} className={`chip ${f.lenses.includes(l) ? "on" : ""}`} onClick={() => set({ lenses: toggle(f.lenses, l) })}>{l}</button>
                ))}
              </Section>
            )}
            {options.people.length > 0 && (
              <Section title="People">
                {options.people.map((p) => (
                  <button key={p} className={`chip ${f.people.includes(p) ? "on" : ""}`} onClick={() => set({ people: toggle(f.people, p) })}>Person {p}</button>
                ))}
              </Section>
            )}
            {options.tags.length > 0 && (
              <Section title="Content">
                {options.tags.slice(0, 30).map((t) => (
                  <button key={t} className={`chip ${f.tags.includes(t) ? "on" : ""}`} onClick={() => set({ tags: toggle(f.tags, t) })}>{t}</button>
                ))}
              </Section>
            )}
            {extra > 0 && (
              <button className="btn ghost small" style={{ marginTop: 12 }} onClick={() => setFilters({ ...emptyFilters, flag: f.flag, collapseBursts: f.collapseBursts })}>
                Clear filters
              </button>
            )}
          </div>
        )}
      </div>

      <span className="engraved group-label collapse-label">Bursts</span>
      <Key label="Stack" lamp="white" lit={f.collapseBursts} title="Show one tile per burst (the best shot)"
        onClick={() => set({ collapseBursts: !f.collapseBursts })} className="small" />

      <div className="spacer" />
      {(extra > 0 || f.collapseBursts) && (
        <Readout label="Showing" slot={String(counts[f.flag]).length * 2 + 3}>{shown.toLocaleString()} / {counts[f.flag].toLocaleString()}</Readout>
      )}
      {showThumbSize && (
        <label className="size-slider" title="Thumbnail size">
          <span className="engraved">Size</span>
          <input
            type="range" min={120} max={380} step={20} value={thumbSize}
            onChange={(e) => setThumbSize(Number(e.target.value))}
            aria-label="Thumbnail size"
          />
        </label>
      )}
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="field" style={{ marginTop: 14 }}>
      <label>{title}</label>
      <div className="chip-row">{children}</div>
    </div>
  );
}
