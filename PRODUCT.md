# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

(A Tauri desktop app for Windows and macOS; the interface is web code in a native window, 1400×900 by default, 960×640 minimum.)

## Users

Working professional photographers after a job: wedding, event and portrait shooters, many of whom also shoot video. They come home or back to the studio with one or more full cards and need to get from card to a culled set quickly, often late at night, on a laptop or in a dim edit room, for hundreds to thousands of frames in a sitting. Keyboard-driven, repetitive, judgement-heavy work.

## Product Purpose

Safelight imports photos and clips from camera cards, verifies every copy, sorts them into dated project folders, lets the photographer cull (pick, reject, rate) and hands the keepers to Lightroom Classic or DaVinci Resolve. Success is a card emptied safely and a shoot culled in one sitting, with nothing lost and nothing re-done in the editor.

## Positioning

- **Cull while it copies:** culling starts before the import has finished.
- **Local AI suggestions:** sharpness, missed focus, motion blur, closed eyes, exposure, tilted horizons, bursts with the best shot marked, content tags, an aesthetic score and people grouping, all on the computer, nothing uploaded. It only suggests; the photographer applies and can undo.
- **Safety of the copy:** checksum-verified copies, optional verified backup to a second drive, duplicates skipped, the card is never changed, rejects go to a `_Rejected/` folder before any deletion.
- **Two editors:** hand-off to Lightroom Classic (ratings, picks, keywords, copyright via XMP) and DaVinci Resolve Studio (project with bins per day and camera).

## Operating Context

Card reader or folder → project named `YYYY-MM-DD_Name` under a projects folder, sorted `<date>/<camera>/`, videos in `Video/`. Ratings and picks live in `.xmp` sidecars and `<project>/.grabit/`. Keyboard shortcuts drive culling: P pick, X reject, U unflag, 0–5 stars, arrows, E single photo, G grid, C compare, Z 100 % zoom, I info, Ctrl+Z undo.

## Capabilities and Constraints

- Screens: Home (cards, imports, projects), Cull (grid, single photo with zoom, compare 2–4, filmstrip, filters, info panel), dialogs for import, report, settings, send, confirmations.
- Must stay dark: culling needs a neutral dark surround.
- Photos stay neutral: no coloured tint, glow or coloured surface adjacent to images; colour judgement must stay accurate.
- Pick is green, reject is red, stars are the rating vocabulary shared with Lightroom.
- Builds are not code-signed yet.

## Brand Commitments

The name is Safelight (renamed from GrabIt; internal identifiers such as the `.grabit/` project folder keep the old name for compatibility). The app icon is the current-frame crop marks around a lit pick lamp on a graphite tile; its source is `app-icon.svg`.

## Evidence on Hand

README feature list; sample cards in `test-fixtures/` (Canon CR2/CR3, Sony ARW and MP4, face JPEGs). No testimonials, user counts or benchmarks exist; do not invent them.

## Product Principles

1. The photo is the subject; the interface is the darkroom around it.
2. Never make the photographer wait or wonder: every action is instant and confirmed.
3. Safety is visible: what was copied, verified, backed up or moved is always stated plainly.
4. The AI advises, the photographer decides.
5. Keyboard first, mouse always possible.
