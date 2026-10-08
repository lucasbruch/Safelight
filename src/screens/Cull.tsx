import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, formatBytes, formatShutter, on, plural } from "../api";
import { Key, Lamp, Legend, Readout } from "../components/Deck";
import type { Item, Progress } from "../types";
import type { ToastMsg } from "../components/Toast";
import { Icon } from "../components/Icon";
import { useEscape } from "../components/useEscape";
import FilterBar from "../cull/FilterBar";
import Grid from "../cull/Grid";
import InfoPanel from "../cull/InfoPanel";
import SendDialog from "../cull/SendDialog";
import { Compare, ZoomImage, Center } from "../cull/Viewer";
import { applyFilters, emptyFilters, filterOptions, Filters, FlagFilter } from "../cull/filters";
import { FlagBadge, Thumb } from "../cull/bits";
import { createSaveQueue } from "../cull/saveQueue";

type View = "grid" | "loupe" | "compare";
type Undo = { ids: number[]; field: "flag" | "rating"; before: Map<number, number> };
type Hud = { key: number; field: "flag" | "rating"; value: number; count: number };

interface Props {
  root: string;
  progress: Progress | null;
  onBack: () => void;
  notify: (t: ToastMsg) => void;
}

export default function Cull({ root, progress, onBack, notify }: Props) {
  const [name, setName] = useState("");
  const [items, setItems] = useState<Item[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [filters, setFilters] = useState<Filters>(emptyFilters);
  const [currentId, setCurrentId] = useState<number | null>(null);
  const [selection, setSelection] = useState<Set<number>>(new Set());
  const [view, setView] = useState<View>("grid");
  const [zoom, setZoom] = useState(false);
  const [center, setCenter] = useState<Center>({ x: 0.5, y: 0.5 });
  const [thumbSize, setThumbSize] = useState(() => Number(pref("thumb") ?? 220));
  const [autoAdvance, setAutoAdvance] = useState(() => pref("advance") !== "0");
  const [showInfo, setShowInfo] = useState(() => pref("info") !== "0");
  const [sending, setSending] = useState(false);
  const [confirm, setConfirm] = useState<null | "move" | "trash" | "accept">(null);
  const [hud, setHud] = useState<Hud | null>(null);
  const [showKeys, setShowKeys] = useState(false);
  const cols = useRef(4);
  const undo = useRef<Undo[]>([]);
  const anchor = useRef<number | null>(null);
  const setCols = useCallback((c: number) => void (cols.current = c), []);
  const clearHud = useCallback(() => setHud(null), []);
  // The on-screen key deck mirrors the physical keyboard: the matching cap goes down briefly.
  const [down, setDown] = useState<string | null>(null);
  const downTimer = useRef<number>(0);
  const press = useCallback((k: string) => {
    setDown(k);
    clearTimeout(downTimer.current);
    downTimer.current = window.setTimeout(() => setDown(null), 140);
  }, []);
  const closeKeys = useCallback(() => setShowKeys(false), []);

  // ---- data --------------------------------------------------------------
  // Every change to the project goes through one queue, in the order you made it.
  const [{ serial, settled, save: queueSave }] = useState(createSaveQueue);

  const reload = useCallback(() => {
    serial(() => api.openProject(root)).then((p) => {
      setName(p.name);
      setItems(p.items);
      setLoaded(true);
      setCurrentId((c) => c ?? p.items.find((i) => !i.movedToRejected)?.id ?? null);
    }, (e) => {
      notify({ text: String(e), bad: true });
      onBack();
    });
  }, [root, notify, onBack, serial]);

  useEffect(reload, [reload]);

  const merge = useCallback((updated: Item[]) => {
    if (!updated.length) return;
    const byId = new Map(updated.map((u) => [u.id, u]));
    setItems((cur) => cur.map((i) => byId.get(i.id) ?? i));
  }, []);

  const save = useCallback(
    async (ids: number[], call: () => Promise<Item[]>) => merge(await queueSave(ids, call)),
    [queueSave, merge],
  );

  // Previews and AI results arrive in the background and may have been read
  // before your latest pick or rating was saved: take everything from them
  // except what you set yourself, so a keypress is never visually undone.
  const mergeBackground = useCallback((updated: Item[]) => {
    if (!updated.length) return;
    const byId = new Map(updated.map((u) => [u.id, u]));
    setItems((cur) => cur.map((i) => {
      const u = byId.get(i.id);
      return u ? { ...u, flag: i.flag, rating: i.rating, movedToRejected: i.movedToRejected, tags: i.tags } : i;
    }));
  }, []);

  useEffect(() => {
    const subs = [
      on("item-added", (e) => {
        if (e.projectRoot !== root) return;
        setItems((cur) => (cur.some((i) => i.id === e.item.id) ? cur : sortItems([...cur, e.item])));
        setCurrentId((c) => c ?? e.item.id);
      }),
      on("item-updated", (e) => e.projectRoot === root && mergeBackground([e.item])),
      on("items-updated", (e) => e.projectRoot === root && mergeBackground(e.items)),
    ];
    return () => void subs.forEach((s) => s.then((u) => u()));
  }, [root, mergeBackground]);

  useEffect(() => void safeSet("safelight.thumb", String(thumbSize)), [thumbSize]);
  useEffect(() => void safeSet("safelight.advance", autoAdvance ? "1" : "0"), [autoAdvance]);
  useEffect(() => void safeSet("safelight.info", showInfo ? "1" : "0"), [showInfo]);

  // ---- derived -------------------------------------------------------------
  const shown = useMemo(() => applyFilters(items, filters), [items, filters]);
  const options = useMemo(() => filterOptions(items), [items]);
  const counts = useMemo(() => {
    const live = items.filter((i) => !i.movedToRejected);
    return {
      all: live.length,
      picks: live.filter((i) => i.flag === 1).length,
      unflagged: live.filter((i) => i.flag === 0).length,
      rejects: items.filter((i) => i.flag === -1).length,
    } as Record<FlagFilter, number>;
  }, [items]);
  const current = useMemo(() => items.find((i) => i.id === currentId) ?? null, [items, currentId]);
  const idx = shown.findIndex((i) => i.id === currentId);
  const rejectsToMove = items.filter((i) => i.flag === -1 && !i.movedToRejected).length;
  const movedRejects = items.filter((i) => i.movedToRejected).length;
  const aiPending = items.filter((i) => i.kind === "photo" && i.previewState === 1 && i.aiState === 0).length;
  const suggestions = useMemo(() => {
    const open = items.filter((i) => i.flag === 0 && !i.movedToRejected && i.ai);
    return { keep: open.filter((i) => i.ai!.suggestion === 1), reject: open.filter((i) => i.ai!.suggestion === -1) };
  }, [items]);

  // Keep the current photo inside the filtered list.
  useEffect(() => {
    if (!shown.length) return;
    if (currentId == null || !shown.some((i) => i.id === currentId)) setCurrentId(shown[0].id);
  }, [shown, currentId]);

  const compareItems = useMemo(() => {
    const sel = shown.filter((i) => selection.has(i.id));
    if (sel.length >= 2) return sel.slice(0, 4);
    if (current?.ai?.burstId != null) {
      const burst = items.filter((i) => i.ai?.burstId === current.ai!.burstId && !i.movedToRejected);
      if (burst.length >= 2) return burst.slice(0, 4);
    }
    return shown.slice(Math.max(0, idx), Math.max(0, idx) + 2);
  }, [shown, selection, current, items, idx]);

  // ---- actions --------------------------------------------------------------
  const targets = useCallback((): number[] => {
    if (currentId == null) return [];
    if (selection.size > 1 && selection.has(currentId)) return [...selection];
    return [currentId];
  }, [currentId, selection]);

  const move = useCallback((delta: number) => {
    if (!shown.length) return;
    const list = view === "compare" ? compareItems : shown;
    const i = list.findIndex((x) => x.id === currentId);
    const next = list[Math.min(list.length - 1, Math.max(0, (i < 0 ? 0 : i) + delta))];
    if (next) {
      setCurrentId(next.id);
      if (view !== "compare") setSelection(new Set());
      anchor.current = next.id;
    }
  }, [shown, compareItems, currentId, view]);

  const snapshot = useCallback((ids: number[], field: "flag" | "rating"): Undo => {
    const byId = new Map(items.map((i) => [i.id, i]));
    return { ids, field, before: new Map(ids.map((id) => [id, byId.get(id)?.[field] ?? 0])) };
  }, [items]);

  const pushUndo = (u: Undo) => {
    undo.current.push(u);
    if (undo.current.length > 100) undo.current.shift();
  };

  const apply = useCallback(async (field: "flag" | "rating", value: number, ids = targets(), recordUndo = true) => {
    if (!ids.length) return;
    if (recordUndo) pushUndo(snapshot(ids, field));
    // Optimistic: the UI never waits for disk.
    const idSet = new Set(ids);
    setItems((cur) => cur.map((i) => (idSet.has(i.id) ? { ...i, [field]: value } : i)));
    // Confirm what just happened, since auto-advance moves on before you can look.
    setHud({ key: Date.now(), field, value, count: ids.length });
    // Advance now, not after the save: the next key press must land on the next photo.
    if (autoAdvance && ids.length === 1 && view !== "compare") move(1);
    try {
      await save(ids, () => (field === "flag" ? api.setFlag(root, ids, value) : api.setRating(root, ids, value)));
    } catch (e) {
      notify({ text: String(e), bad: true });
      reload();
    }
  }, [targets, snapshot, root, save, notify, autoAdvance, move, view, reload]);

  const editTags = useCallback(async (add: string[], remove: string[]) => {
    const ids = targets();
    if (!ids.length) return;
    try {
      await save(ids, () => api.editTags(root, ids, add, remove));
    } catch (e) {
      notify({ text: String(e), bad: true });
      reload();
    }
  }, [targets, root, save, notify, reload]);

  const tagTargets = useMemo(() => {
    const ids = new Set(targets());
    return items.filter((i) => ids.has(i.id));
  }, [targets, items]);

  const undoLast = useCallback(async () => {
    const u = undo.current.pop();
    if (!u) return;
    setItems((cur) => cur.map((i) => (u.before.has(i.id) ? { ...i, [u.field]: u.before.get(i.id)! } : i)));
    setCurrentId(u.ids[0]);
    // One call per distinct old value, not one per photo.
    const byValue = new Map<number, number[]>();
    for (const [id, v] of u.before) byValue.set(v, [...(byValue.get(v) ?? []), id]);
    try {
      for (const [v, ids] of byValue) {
        await save(ids, () => (u.field === "flag" ? api.setFlag(root, ids, v) : api.setRating(root, ids, v)));
      }
      notify({ text: "Undone", ms: 1500 });
    } catch (e) {
      notify({ text: String(e), bad: true });
      reload();
    }
  }, [root, save, notify, reload]);

  const clickItem = (id: number, e: React.MouseEvent) => {
    if (e.shiftKey && anchor.current != null) {
      const a = shown.findIndex((i) => i.id === anchor.current), b = shown.findIndex((i) => i.id === id);
      const [lo, hi] = a < b ? [a, b] : [b, a];
      setSelection(new Set(shown.slice(lo, hi + 1).map((i) => i.id)));
    } else if (e.ctrlKey || e.metaKey) {
      setSelection((s) => {
        const n = new Set(s.size ? s : currentId != null ? [currentId] : []);
        if (n.has(id)) n.delete(id);
        else n.add(id);
        return n;
      });
      anchor.current = id;
    } else {
      setSelection(new Set());
      anchor.current = id;
    }
    setCurrentId(id);
  };

  // ---- keyboard ------------------------------------------------------------
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      // Any open dialog (including ones owned by App) takes the keyboard.
      if (t.tagName === "INPUT" || t.tagName === "SELECT" || t.tagName === "TEXTAREA" || document.querySelector(".overlay")) return;
      const mod = e.ctrlKey || e.metaKey;
      const k = e.key.toLowerCase();
      let handled = true;
      if (k === "arrowright") move(1);
      else if (k === "arrowleft") move(-1);
      else if (k === "arrowdown") move(view === "grid" ? cols.current : 1);
      else if (k === "arrowup") move(view === "grid" ? -cols.current : -1);
      else if (k === "home") shown[0] && setCurrentId(shown[0].id);
      else if (k === "end") shown.length && setCurrentId(shown[shown.length - 1].id);
      else if (mod && k === "z") undoLast();
      else if (mod && k === "a") setSelection(new Set(shown.map((i) => i.id)));
      else if (mod) handled = false;
      else if (k === "p") { press("p"); apply("flag", 1); }
      else if (k === "x" || k === "delete" || k === "backspace") { press("x"); apply("flag", -1); }
      else if (k === "u") { press("u"); apply("flag", 0); }
      else if (/^[0-5]$/.test(k)) { press(k); apply("rating", Number(k)); }
      else if (k === "g") { press("g"); setView("grid"); setZoom(false); }
      else if (k === "e" || k === "enter") { press("e"); setView("loupe"); }
      else if (k === "c") { press("c"); setView("compare"); setZoom(false); }
      else if (k === "z") {
        if (view === "grid") setView("loupe");
        setZoom((z) => !z);
        setCenter({ x: 0.5, y: 0.5 });
      }
      else if (k === " ") { setView((v) => (v === "grid" ? "loupe" : "grid")); setZoom(false); }
      else if (k === "i") { press("i"); setShowInfo((s) => !s); }
      else if (k === "t" && currentId != null) {
        press("t");
        setShowInfo(true);
        // Once the panel is on screen.
        requestAnimationFrame(() => document.getElementById("tag-input")?.focus());
      }
      else if (e.key === "?" || (e.shiftKey && e.code === "Slash")) { press("?"); setShowKeys((s) => !s); }
      else if (k === "escape") {
        if (showKeys) setShowKeys(false);
        else if (zoom) setZoom(false);
        else if (view !== "grid") setView("grid");
        else setSelection(new Set());
      }
      else handled = false;
      if (handled) e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [move, apply, undoLast, view, zoom, shown, showKeys, press, currentId]);

  // ---- bulk actions ----------------------------------------------------------
  const doMoveRejects = async () => {
    setConfirm(null);
    try {
      const r = await serial(() => api.moveRejects(root));
      if (r.failed.length) {
        notify({
          text: `Moved ${plural(r.moved, "reject")}. ${plural(r.failed.length, "photo")} couldn't be moved (is it open in another app?): ${r.failed[0]}`,
          bad: true,
        });
      } else {
        notify({ text: `Moved ${plural(r.moved, "reject")} into the _Rejected folder. Un-reject one anytime to bring it back.` });
      }
      reload();
    } catch (e) {
      notify({ text: String(e), bad: true });
    }
  };
  const doTrash = async () => {
    setConfirm(null);
    try {
      const n = await serial(() => api.trashRejects(root));
      notify({ text: `Sent ${plural(n, "reject")} to the ${navigator.platform.startsWith("Mac") ? "Trash" : "Recycle Bin"}.` });
      reload();
    } catch (e) {
      notify({ text: String(e), bad: true });
    }
  };
  const acceptSuggestions = async (which: "both" | "keep" | "reject") => {
    setConfirm(null);
    const keep = which !== "reject" ? suggestions.keep.map((i) => i.id) : [];
    const reject = which !== "keep" ? suggestions.reject.map((i) => i.id) : [];
    if (!keep.length && !reject.length) return;
    // One undo step for the whole batch, as the message promises.
    pushUndo(snapshot([...keep, ...reject], "flag"));
    if (keep.length) await apply("flag", 1, keep, false);
    if (reject.length) await apply("flag", -1, reject, false);
    notify({ text: "Applied. Press Ctrl+Z to undo." });
  };

  // ---- render --------------------------------------------------------------
  return (
    <div className="cull">
      <header className="topbar">
        <button className="btn ghost icon-btn" onClick={onBack} title="All projects" aria-label="Back"><Icon name="back" /></button>
        <h1 title={root}>{name}</h1>
        <div className="readouts">
          <Readout label="Items" slot={5}>{counts.all.toLocaleString()}</Readout>
          <Readout label="Picks" lamp="pick" lit={counts.picks > 0} slot={5}>{counts.picks.toLocaleString()}</Readout>
          <Readout label="Rejects" lamp="reject" lit={counts.rejects > 0} slot={5}>{counts.rejects.toLocaleString()}</Readout>
        </div>
        <div className="spacer" />
        <div className="key-row" role="group" aria-label="View">
          <Key legend="G" label={<Icon name="grid" size={15} />} lamp="white" lit={view === "grid"} pressed={down === "g"} title="Grid (G)"
            onClick={() => { setView("grid"); setZoom(false); }} />
          <Key legend="E" label={<Icon name="loupe" size={15} />} lamp="white" lit={view === "loupe"} pressed={down === "e"} title="Single photo (E)"
            onClick={() => setView("loupe")} />
          <Key legend="C" label={<Icon name="compare" size={15} />} lamp="white" lit={view === "compare"} pressed={down === "c"} title="Compare (C)"
            onClick={() => { setView("compare"); setZoom(false); }} />
        </div>
        {(suggestions.keep.length > 0 || suggestions.reject.length > 0) && (
          <button className="btn" onClick={() => setConfirm("accept")} title="Apply the AI's suggestions to photos you haven't flagged yet" aria-label="AI suggestions">
            <Legend>AI</Legend><span className="collapse-label">Suggestions</span>
          </button>
        )}
        {rejectsToMove > 0 && (
          <button className="btn" onClick={() => setConfirm("move")} title={`Move ${plural(rejectsToMove, "reject")} into the _Rejected folder`}>
            <Icon name="archive" size={15} />
            <span>Move <span className="num">{rejectsToMove.toLocaleString()}</span><span className="collapse-label"> {rejectsToMove === 1 ? "reject" : "rejects"} away</span></span>
          </button>
        )}
        {rejectsToMove === 0 && movedRejects > 0 && filters.flag === "rejects" && (
          <button className="btn danger" onClick={() => setConfirm("trash")}><Icon name="trash" size={15} /> Empty rejects</button>
        )}
        <button className="btn primary" onClick={() => setSending(true)}><Icon name="send" size={15} /> Send…</button>
        <Key legend="I" label={<Icon name="info" size={16} />} lamp="white" lit={showInfo} pressed={down === "i"} title="Info panel (I)" onClick={() => setShowInfo((s) => !s)} />
      </header>

      {progress && (
        <div className="import-strip">
          <Lamp color="busy" blink />
          <span className="engraved">Ingest</span>
          <div className="progress thin"><div style={{ width: `${progress.bytesTotal ? (progress.bytesDone / progress.bytesTotal) * 100 : 0}%` }} /></div>
          <Readout label="Verified" lamp="pick" lit={progress.done - progress.failed > 0} slot={String(progress.total).length * 2 + 3}
            title="Copied and checked against the card">{(progress.done - progress.failed).toLocaleString()} / {progress.total.toLocaleString()}</Readout>
          <Readout label="Copied" slot={15}>{formatBytes(progress.bytesDone)} / {formatBytes(progress.bytesTotal)}</Readout>
          {progress.skipped > 0 && <Readout label="Skipped" title="Already imported before">{progress.skipped.toLocaleString()}</Readout>}
          <Readout label="Failed" lamp="reject" lit={progress.failed > 0} slot={3}
            title={progress.failed ? "These files stay safe on the card; the report lists them" : "No copy has failed"}>{progress.failed.toLocaleString()}</Readout>
          <span className="muted nowrap collapse-label">Cull now, it keeps copying.</span>
          <div className="spacer" />
          <button className="btn small ghost" onClick={() => api.cancelImport()}>Stop import</button>
        </div>
      )}

      <FilterBar
        filters={filters}
        setFilters={setFilters}
        options={options}
        counts={counts}
        shown={shown.length}
        thumbSize={thumbSize}
        setThumbSize={setThumbSize}
        showThumbSize={view === "grid"}
      />

      <div className="cull-main">
        <div className="stage">
          {loaded && shown.length === 0 ? (
            <div className="stage-empty">
              {items.length === 0 ? (
                <p className="muted">{progress ? "Your first photos are on their way…" : "This project is empty."}</p>
              ) : (
                <>
                  <p className="muted">No photos match these filters.</p>
                  <button className="btn small" onClick={() => setFilters(emptyFilters)}>Show everything</button>
                </>
              )}
            </div>
          ) : view === "grid" ? (
            <Grid
              root={root}
              items={shown}
              currentId={currentId}
              selection={selection}
              thumbSize={thumbSize}
              onClick={clickItem}
              onOpen={(id) => { setCurrentId(id); setView("loupe"); }}
              onColumns={setCols}
            />
          ) : view === "loupe" && current ? (
            <>
              <ZoomImage
                key={current.id}
                root={root}
                item={current}
                zoom={zoom}
                center={center}
                onCenter={setCenter}
                onToggleZoom={(c) => { setCenter(c); setZoom((z) => !z); }}
              />
              <Slate item={current} zoom={zoom} />
              <Filmstrip root={root} items={shown} currentId={currentId} onSelect={setCurrentId} />
            </>
          ) : view === "compare" ? (
            <Compare root={root} items={compareItems} currentId={currentId} onSelect={setCurrentId} zoom={zoom} setZoom={setZoom} />
          ) : null}

          {hud && <ActionHud key={hud.key} hud={hud} onDone={clearHud} />}

          <div className="bottombar">
            <div className="key-row">
              <Key legend="P" label="Pick" lamp="pick" lit={current?.flag === 1} pressed={down === "p"} title="Pick (P)"
                onClick={() => apply("flag", current?.flag === 1 ? 0 : 1)} />
              <Key legend="X" label="Reject" lamp="reject" lit={current?.flag === -1} pressed={down === "x"} title="Reject (X)"
                onClick={() => apply("flag", current?.flag === -1 ? 0 : -1)} />
            </div>
            <StarKeys value={current?.rating ?? 0} down={down} onRate={(r) => apply("rating", current?.rating === r ? 0 : r)} />
            <Key label={<span>Auto<span className="collapse-label">-advance</span></span>} lamp="white" lit={autoAdvance} title="Jump to the next photo after you pick, reject or rate"
              onClick={() => setAutoAdvance((a) => !a)} />
            <div className="spacer" />
            {aiPending > 0 && (
              <Readout label="AI queue" lamp="busy" title="The AI helper is still looking at these photos" slot={5}>{aiPending.toLocaleString()}</Readout>
            )}
            {idx >= 0 && <Readout label="Frame" title="Position in the photos shown" slot={String(shown.length).length * 2 + 3}>{(idx + 1).toLocaleString()} / {shown.length.toLocaleString()}</Readout>}
            <div className="shortcuts-anchor">
              <Key legend="?" label="Keys" lamp="white" lit={showKeys} pressed={down === "?"} title="All keyboard shortcuts (?)" onClick={() => setShowKeys((s) => !s)} />
              {showKeys && <Shortcuts onClose={closeKeys} />}
            </div>
          </div>
        </div>
        {showInfo && (
          <InfoPanel
            root={root}
            item={current}
            count={selection.size}
            tagTargets={tagTargets}
            tagSuggestions={[...options.userTags, ...options.tags]}
            onEditTags={editTags}
          />
        )}
      </div>

      {sending && <SendDialog root={root} items={items} selection={selection} settled={settled} onClose={() => setSending(false)} />}

      {confirm === "move" && (
        <Confirm
          title={`Move ${plural(rejectsToMove, "reject")} away?`}
          body="They go into a _Rejected folder inside this project, out of your way but not deleted. Un-reject one anytime to bring it back."
          ok="Move them" onOk={doMoveRejects} onCancel={() => setConfirm(null)}
        />
      )}
      {confirm === "trash" && (
        <Confirm
          title={`Empty ${plural(movedRejects, "reject")}?`}
          body={`The _Rejected folder goes to your ${navigator.platform.startsWith("Mac") ? "Trash" : "Recycle Bin"}. You can still restore it from there until you empty it. Backup copies aren't touched.`}
          ok="Empty rejects" danger onOk={doTrash} onCancel={() => setConfirm(null)}
        />
      )}
      {confirm === "accept" && (
        <AcceptDialog onClose={() => setConfirm(null)}>
        <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && setConfirm(null)}>
          <div className="modal" role="dialog" aria-modal="true" aria-label="Use the AI's suggestions">
            <h2>Use the AI's suggestions?</h2>
            <div className="muted">Only for photos you haven't flagged yet. Your own picks and rejects are never changed.</div>
            <div className="summary-box">
              <div><b style={{ color: "var(--pick)" }}>{suggestions.keep.length}</b> look like keepers: sharp, eyes open, well exposed, or the best of a burst.</div>
              <div style={{ marginTop: 6 }}><b style={{ color: "var(--reject)" }}>{suggestions.reject.length}</b> look like rejects: blurry, eyes closed, badly exposed, or a weaker burst shot.</div>
            </div>
            <div className="actions">
              <button className="btn ghost" onClick={() => setConfirm(null)}>Cancel</button>
              <button className="btn" disabled={!suggestions.reject.length} onClick={() => acceptSuggestions("reject")}>Only rejects</button>
              <button className="btn" disabled={!suggestions.keep.length} onClick={() => acceptSuggestions("keep")}>Only picks</button>
              <button className="btn primary" onClick={() => acceptSuggestions("both")}>Apply both</button>
            </div>
          </div>
        </div>
        </AcceptDialog>
      )}
    </div>
  );
}

function Filmstrip({ root, items, currentId, onSelect }: { root: string; items: Item[]; currentId: number | null; onSelect: (id: number) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const idx = items.findIndex((i) => i.id === currentId);
  // Only render a window around the current photo; long shoots have thousands.
  const start = Math.max(0, idx - 30);
  const slice = items.slice(start, start + 61);
  useEffect(() => {
    ref.current?.querySelector(".current")?.scrollIntoView({ inline: "center", block: "nearest" });
  }, [currentId]);
  return (
    <div className="filmstrip" ref={ref}>
      {slice.map((it) => (
        <div key={it.id} className={`thumb ${it.id === currentId ? "current" : ""} ${it.flag === -1 ? "rejected" : ""}`} onClick={() => onSelect(it.id)} title={it.fileName}>
          <Thumb root={root} item={it} />
          <FlagBadge flag={it.flag} small />
        </div>
      ))}
    </div>
  );
}

const SHORTCUTS: [string[], string][] = [
  [["P"], "Pick"],
  [["X"], "Reject"],
  [["U"], "Clear pick or reject"],
  [["1", "5"], "Stars, 0 clears"],
  [["←", "→"], "Previous / next"],
  [["↑", "↓"], "Row up / down"],
  [["G"], "Grid"],
  [["E"], "Single photo"],
  [["Space"], "Grid or single photo"],
  [["C"], "Compare"],
  [["Z"], "Zoom to 100 %"],
  [["I"], "Info panel"],
  [["T"], "Add a tag"],
  [["Ctrl", "A"], "Select all shown"],
  [["Ctrl", "Z"], "Undo"],
  [["Esc"], "Back out, clear selection"],
];

function Shortcuts({ onClose }: { onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const close = (e: MouseEvent) => {
      const anchor = ref.current?.parentElement;
      if (anchor && !anchor.contains(e.target as Node)) onClose();
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [onClose]);
  return (
    <div className="popover shortcuts" ref={ref} role="dialog" aria-label="Keyboard shortcuts">
      <dl>
        {SHORTCUTS.map(([keys, what]) => (
          <div key={what}>
            <dt>
              {keys.map((k, i) => (
                <span key={k}>{i > 0 && <span className="faint">{keys[0] === "Ctrl" ? "+" : "–"}</span>}<span className="kbd">{k}</span></span>
              ))}
            </dt>
            <dd>{what}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

/** Five star keys; each lamp lights up to the rating, hovering previews the rating a click would set. */
function StarKeys({ value, down, onRate }: { value: number; down: string | null; onRate: (r: number) => void }) {
  const [hover, setHover] = useState(0);
  const lit = hover || value;
  return (
    <div className={`key-row stars-keys ${hover ? "previewing" : ""}`} role="group" aria-label="Rating" onMouseLeave={() => setHover(0)}>
      {[1, 2, 3, 4, 5].map((r) => (
        <span key={r} onMouseEnter={() => setHover(r)}>
          <Key
            legend={String(r)}
            lamp="star"
            lit={r <= lit}
            pressed={down === String(r) || (down === "0" && r <= value)}
            title={value === r ? `Clear rating (${r} or 0)` : `${r} star${r > 1 ? "s" : ""} (${r})`}
            onClick={() => onRate(r)}
          />
        </span>
      ))}
    </div>
  );
}

/** The credits line under the photo in single view: what, when and how it was shot. */
function Slate({ item, zoom }: { item: Item; zoom: boolean }) {
  const m = item.meta;
  const exposure = [formatShutter(m.shutter), m.aperture && `f/${m.aperture}`, m.iso && `ISO\u00a0${m.iso}`, m.focal && `${Math.round(m.focal)}\u00a0mm`].filter(Boolean);
  return (
    <div className="slate">
      <b>{item.fileName}</b>
      {exposure.map((e) => <span key={String(e)} className="num">{e}</span>)}
      {m.lens && <span className="muted">{m.lens}</span>}
      <span className="muted">{item.camera}</span>
      <div className="spacer" />
      <span className="engraved">{zoom ? "100 %" : "Fit"}</span>
    </div>
  );
}

/** A brief readout over the photo of the flag or rating just set. */
function ActionHud({ hud, onDone }: { hud: Hud; onDone: () => void }) {
  useEffect(() => {
    const t = setTimeout(onDone, 900);
    return () => clearTimeout(t);
  }, [onDone]);
  let body: React.ReactNode;
  if (hud.field === "flag") {
    body = hud.value === 1 ? <><Lamp color="pick" /> Pick</>
      : hud.value === -1 ? <><Lamp color="reject" /> Reject</>
      : <><Lamp color="white" lit={false} /> Unflagged</>;
  } else {
    body = hud.value === 0 ? <><Lamp color="star" lit={false} /> No rating</> : (
      <>
        <span className="hud-lamps">{[1, 2, 3, 4, 5].map((i) => <Lamp key={i} color="star" lit={i <= hud.value} />)}</span>
        {plural(hud.value, "star")}
      </>
    );
  }
  return (
    <div className="hud" role="status">
      {body}
      {hud.count > 1 && <span className="hud-count num">× {hud.count.toLocaleString()}</span>}
    </div>
  );
}
function Confirm({ title, body, ok, danger, onOk, onCancel }: { title: string; body: string; ok: string; danger?: boolean; onOk: () => void; onCancel: () => void }) {
  useEscape(onCancel);
  return (
    <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && onCancel()}>
      <div className="modal" role="alertdialog" aria-modal="true" aria-label={title}>
        <h2>{title}</h2>
        <div className="muted">{body}</div>
        <div className="actions">
          <button className="btn ghost" onClick={onCancel}>Cancel</button>
          <button className={`btn ${danger ? "danger" : "primary"}`} onClick={onOk} autoFocus>{ok}</button>
        </div>
      </div>
    </div>
  );
}

function AcceptDialog({ onClose, children }: { onClose: () => void; children: React.ReactNode }) {
  useEscape(onClose);
  return <>{children}</>;
}

function sortItems(items: Item[]): Item[] {
  return items.sort((a, b) => (a.capturedAt ?? "￿").localeCompare(b.capturedAt ?? "￿") || a.fileName.localeCompare(b.fileName));
}

/** A per-viewer preference, read from its pre-rename key if it hasn't been saved since. */
function pref(name: string) {
  return safeGet(`safelight.${name}`) ?? safeGet(`grabit.${name}`);
}
function safeGet(k: string): string | null {
  try { return localStorage.getItem(k); } catch { return null; }
}
function safeSet(k: string, v: string) {
  try { localStorage.setItem(k, v); } catch { /* per-viewer convenience only */ }
}
