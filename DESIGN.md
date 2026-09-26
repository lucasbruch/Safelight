---
name: Safelight
description: The control surface of a dim grading suite, for importing and culling a shoot at night.
colors:
  cap-light: "#e3e5e9"
  pick: "#3fd18a"
  reject: "#ff4d4d"
  star: "#ffb020"
  white-lamp: "#eef1f5"
  well: "#0b0b0c"
  frame: "#0f0f10"
  ground: "#111214"
  deck: "#17181b"
  key: "#1f2024"
  key-hi: "#27282d"
  key-down: "#1a1b1e"
  line: "#26282c"
  line-strong: "#34363c"
  lamp-off: "#33353b"
  text: "#e6e7ea"
  muted: "#a3a7af"
  faint: "#878b93"
typography:
  display:
    fontFamily: "Barlow, Segoe UI, -apple-system, system-ui, sans-serif"
    fontSize: "28px"
    fontWeight: 600
    lineHeight: 1.1
    letterSpacing: "-0.01em"
    fontFeature: "\"tnum\""
  headline:
    fontFamily: "Barlow, Segoe UI, -apple-system, system-ui, sans-serif"
    fontSize: "20px"
    fontWeight: 600
    lineHeight: 1.3
    letterSpacing: "-0.005em"
  title:
    fontFamily: "Barlow, Segoe UI, -apple-system, system-ui, sans-serif"
    fontSize: "15px"
    fontWeight: 600
    lineHeight: 1.3
  body:
    fontFamily: "Barlow, Segoe UI, -apple-system, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.45
  readout:
    fontFamily: "Barlow, Segoe UI, -apple-system, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 500
    lineHeight: 1.1
    fontFeature: "\"tnum\""
  label:
    fontFamily: "Barlow Semi Condensed, Barlow, Segoe UI, system-ui, sans-serif"
    fontSize: "11px"
    fontWeight: 600
    lineHeight: 1
    letterSpacing: "0.12em"
rounded:
  plate: "3px"
  hardware: "4px"
  panel: "8px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "14px"
  lg: "24px"
  xl: "40px"
components:
  key:
    backgroundColor: "{colors.key}"
    textColor: "{colors.muted}"
    rounded: "{rounded.hardware}"
    padding: "0 10px"
    height: "34px"
  key-hover:
    backgroundColor: "{colors.key-hi}"
  key-pressed:
    backgroundColor: "{colors.key-down}"
  key-small:
    height: "30px"
  button-primary:
    backgroundColor: "{colors.cap-light}"
    textColor: "{colors.ground}"
    rounded: "{rounded.hardware}"
    padding: "0 12px"
    height: "34px"
  button-primary-big:
    backgroundColor: "{colors.cap-light}"
    textColor: "{colors.ground}"
    rounded: "{rounded.hardware}"
    padding: "0 20px"
    height: "42px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.muted}"
    rounded: "{rounded.hardware}"
    padding: "0 12px"
    height: "34px"
  legend:
    backgroundColor: "{colors.well}"
    textColor: "{colors.muted}"
    typography: "{typography.label}"
    rounded: "{rounded.plate}"
    padding: "0 4px"
    height: "17px"
  lamp:
    backgroundColor: "{colors.lamp-off}"
    size: "7px"
  bank-key:
    backgroundColor: "{colors.key}"
    textColor: "{colors.muted}"
    padding: "0 10px"
    height: "30px"
  bank-key-on:
    backgroundColor: "{colors.key-hi}"
    textColor: "{colors.text}"
  chip:
    backgroundColor: "{colors.key}"
    textColor: "{colors.muted}"
    rounded: "{rounded.plate}"
    padding: "0 10px"
    height: "28px"
  input:
    backgroundColor: "{colors.well}"
    textColor: "{colors.text}"
    rounded: "{rounded.hardware}"
    padding: "0 11px"
    height: "38px"
  bay:
    backgroundColor: "{colors.deck}"
    rounded: "{rounded.hardware}"
    padding: "16px 18px 16px 20px"
  project-card:
    backgroundColor: "{colors.deck}"
    rounded: "{rounded.hardware}"
  frame-plate:
    backgroundColor: "rgba(11, 11, 12, .86)"
    textColor: "{colors.text}"
    typography: "{typography.label}"
    rounded: "{rounded.plate}"
    padding: "4px 7px"
  modal:
    backgroundColor: "{colors.deck}"
    rounded: "{rounded.panel}"
    padding: "24px 26px"
    width: "min(560px, calc(100vw - 32px))"
  rail:
    backgroundColor: "{colors.deck}"
    padding: "0 14px"
    height: "56px"
---

# Design System: Safelight

## Overview

**Creative North Star: "The Grading Suite Console"**

Safelight is the control surface of a dim grading suite. The photo sits in a neutral monitor well; everything around it is flat graphite hardware with engraved labels, keycaps that print their own shortcuts, small lamps that light to show state, and fixed readouts that tick over without moving. The photographer reads the deck the way an operator reads hardware: which lamps are lit says what is flagged, readouts say how many, the keys teach the keyboard.

There is no brand hue. Colour is signal only: pick green, reject red, star amber, and a neutral white lamp. It lives in lamps and in the short state text beside them, never on surfaces, borders or fills around a photograph, because colour judgement on the photo must stay accurate. One light-filled cap per surface carries the primary action; every other control is a graphite keycap. The build refuses the category default of blue-accented grey panels and pill buttons.

Density is operational: rails at 56px, keys at 30 to 34px, labels engraved at 11px. The window never drops below 960×640, and every rail stays on one line down to that size.

**Key Characteristics:**
- Graphite ground, deck and monitor well; hairlines, not shadows, separate hardware.
- Colour appears only as lit lamps and state text: pick, reject, star, white.
- One light cap per surface; every other control is a graphite keycap with a lamp and a legend.
- Engraved Barlow Semi Condensed caps for labels, Barlow for everything else, tabular digits in fixed slots.
- The current photo is registered with white corner crop marks.
- The app icon repeats that registration: white crop marks around a lit pick lamp in a monitor well on a graphite tile (`app-icon.svg`, generated with `npx tauri icon app-icon.svg`).

## Colors

A neutral graphite instrument panel with four signal lamps and one light cap; no hue belongs to the brand.

### Primary
- **Backlit Cap** (#e3e5e9): the fill of the single primary action per surface (Send on Cull, Start culling or Import on Home, the confirm button in each dialog), plus progress fill, the count badge and checkbox/range accents. Hover brightens to white, press dims to #cfd2d7.

### Secondary
- **Pick Lamp** (#3fd18a): lit pick lamps, pick state text, verified counts, pick meters, "good" AI plates.
- **Reject Lamp** (#ff4d4d): lit reject lamps, reject state text, failure counts, destructive text on a graphite key, "bad" AI plates.
- **Star Lamp** (#ffb020): star rating glyphs, lit star legends, the busy/ingest lamp (which blinks), warnings.
- **White Lamp** (#eef1f5): neutral lit state (view keys, Info, Keys, Auto-advance), focus outlines, the crop marks on the current photo, the current filmstrip thumb.

### Neutral
- **Monitor Well** (#0b0b0c): the surround behind photos, legend plates, inputs, progress troughs, summary boxes.
- **Frame Backing** (#0f0f10): the letterbox behind each image inside the well.
- **Graphite Ground** (#111214): app background, secondary rails (ingest strip, filter bar, slate, filmstrip, info panel), text on the light cap.
- **Deck** (#17181b): primary rails (top and bottom), bays, project cards, modals, popovers, toasts.
- **Keycap** (#1f2024), **Keycap Lit** (#27282d), **Keycap Down** (#1a1b1e): key at rest, on hover or latched on, and while pressed.
- **Hairline** (#26282c) and **Strong Hairline** (#34363c): dividers and rail edges; key, input and panel borders.
- **Dark Lens** (#33353b): the unlit lamp.
- **Panel Text** (#e6e7ea), **Muted** (#a3a7af), **Faint** (#878b93): values and headings; key labels and secondary text; engraved labels and metadata.

### Named Rules
**The Signal-Only Rule.** Colour appears only in a lamp or in the few words of state text beside it (a flag label, a count, an AI plate's text). No surface, border, fill or tint near a photo is ever coloured, and nothing is coloured for decoration.

**The One Light Cap Rule.** Each surface has exactly one Backlit Cap, for its primary action. Everything else is a graphite keycap, a ghost key, or a bank.

## Typography

**Display Font:** Barlow (bundled 400, 500, 600, 700, with Segoe UI and system-ui fallback)
**Label Font:** Barlow Semi Condensed 600 (bundled, with Barlow fallback)

**Character:** Barlow is a plain, slightly industrial grotesque that reads like printed panel text; its semi-condensed cut, set in tracked caps, is the engraving on the hardware.

### Hierarchy
- **Display** (600, 28px, tabular): big summary numbers in report and send dialogs.
- **Headline** (600, 20px): dialog titles.
- **Title** (600, 15 to 16px): project name in the top rail, bay and card names, info panel filename.
- **Body** (400, 14px, 1.45): all running UI text; 12 to 13.5px for captions, key labels and hints.
- **Readout** (500, 14px, tabular): every count and measured value.
- **Label** (600, 11px, 0.12em tracking, uppercase, Faint): engraved labels on readouts, fields and groups; section and info-block headings step up to 12px, 0.14em in Muted with a hairline running to the right edge. The wordmark and the flag/rating HUD use the same cut at 15px and 14px.

### Named Rules
**The Engraving Rule.** Uppercase tracked text is only the Semi Condensed engraving, and it only names the thing it sits on (a readout, a field, a key group, a panel section). It is never a tagline or a lead-in above a heading.

**The Fixed Slot Rule.** Counts render in tabular figures inside a readout slot sized in `ch` to the widest value it can hold, so digits tick over in place and never shift the rail.

## Layout

The app is a fixed-height console: rails stack top and bottom with hairline edges and the monitor well takes the rest. Cull runs top rail (56px, Deck), optional ingest strip (40px, Ground), filter rail (Ground), the well with grid, loupe or compare (2 to 4 panes, 14px gutters), optional 290px info panel on the right, and the bottom key deck (56px, Deck). Home is a top rail over a scrolling body centred at 1180px max, 30 to 32px padding, 40px between sections; projects are an auto-fill grid of 250px minimum columns with 14px gaps.

Spacing runs on a tight step: 4px between keys in a row, 8px inside keys and between dialog actions, 14px rail padding and readout gutters, 24px dialog padding and action spacing, 40px between Home sections.

Responsive behaviour targets the window, never a phone: the minimum is 960×640 and every rail stays on one line down to it. Below 1240px the first readout in a strip drops. Below 1180px descriptive words on buttons drop and gaps tighten to 8px. Below 1080px keys drop their word labels and keep their shortcut legends. Grid captions shed lens then exposure through container queries as cells narrow.

### Named Rules
**The One Line Rule.** No rail wraps at any window size down to 960px; shed words, never legends or readouts that carry counts.

## Elevation & Depth

Hardware is flat. Rails, bays, cards and keys are separated by tone (Well, Ground, Deck, Keycap) and 1px hairlines, never shadow. Depth appears only where something floats above the console, and lit lamps carry a tiny glow of their own colour.

### Shadow Vocabulary
- **Panel lift** (`box-shadow: 0 24px 64px rgba(0, 0, 0, .6)`): modals, over a 72% black overlay.
- **Float** (`box-shadow: 0 14px 40px rgba(0, 0, 0, .55)`; toasts `0 12px 36px rgba(0, 0, 0, .5)`; HUD `0 10px 28px rgba(0, 0, 0, .5)`): popovers, toasts, the status HUD.
- **Lamp glow** (`box-shadow: 0 0 4px` at the lamp colour, 45 to 60%): lit lamps only.
- **Dark lens** (`box-shadow: inset 0 0 0 1px #4a4d55, inset 0 1px 1px rgba(0, 0, 0, .5)`): unlit lamps.

### Named Rules
**The Flat Hardware Rule.** Anything bolted to the console is flat; only floating panels cast shadow.

## Shapes

Hardware corners are 4px (keys, banks, inputs, bays, project cards); small plates and chips are 3px (legends, frame plates, tags, chips); modal panels are 8px. Lamps are 7px circles (9px in bays and the HUD). Key banks are single bordered strips with hairline dividers between keys. Empty states use a dashed Strong Hairline border. The current photo is bracketed by eight 2px white strokes, 14px long, forming corner crop marks just outside the frame.

## Components

### Keycaps
Flat graphite caps that teach their own shortcut.
- **Shape:** 4px corners, 1px Strong Hairline border, 34px tall (30px small), 10px side padding.
- **Anatomy:** optional lamp, then a legend plate printing the keyboard key, then a Muted label. A lit key turns its label to Panel Text, or to Pick or Reject for those keys.
- **States:** hover to Keycap Lit; press travels 1px down in 60ms and darkens to Keycap Down. A physical keypress depresses the matching on-screen key the same way. Disabled at 42% opacity.
- **Narrow:** below 1080px the label drops and the legend stays.

### Buttons
- **Primary (Backlit Cap):** Backlit Cap fill, Graphite Ground text, weight 600; 42px tall with 20px padding (15px text) on Home. One per surface.
- **Graphite:** same body as a keycap, for secondary actions (Move rejects, dialog alternatives); destructive actions keep the graphite body with Reject text.
- **Ghost:** transparent, Muted text, fills with Keycap on hover; for Cancel, Close, back and icon-only controls.

### Lamps
Unlit lamps are dark filled lenses (Dark Lens with an inset ring); lit lamps fill with their colour and a 4px glow, fading in 120ms. The busy lamp blinks in amber on a 1.1s two-step cycle while ingest runs.

### Readouts
An engraved label over a tabular value, optionally led by a lamp; grouped strips divide readouts with hairlines. Values sit in fixed-width slots.

### Banks
Interlocked key rows for mutually exclusive filters (flag, rating): one bordered strip, 30px keys, exactly one down (Keycap Lit, Panel Text, its lamp lit), with Faint right-aligned counts.

### Chips
Small 3px graphite plates, 28px tall, Muted text; on state brightens to Keycap Lit with a 55% White Lamp border.

### Inputs / Fields
- **Style:** Monitor Well fill, Strong Hairline border, 4px corners, 38px tall (46px, 17px text for the project name).
- **Focus:** border brightens to 60% White Lamp; global focus-visible is a 1.5px White Lamp outline offset 2px.
- **Labels:** engraved, above the field, with Faint 12.5px hints below.

### Bays and Project Cards
Deck panels with a hairline border and 4px corners. A bay is one horizontal row: lamp, name and progress, hairline-divided readouts, action. A project card is a 3:2 cover in the well, a title block, and a three-column readout row (Items, Picks, Rejects) with lamps.

### Frame Plates
Dark, neutral plates laid over a photo (flag, AI verdict, video duration, burst count): 86% Well with a faint white hairline, engraved 10.5px caps. Only their text or lamp carries colour.

### Current Frame
The current cell or compare pane is registered with white corner crop marks that settle in over 180ms; a selected cell gets a 50% white hairline outline. Rejected frames drop to 30% opacity.

### Modals
Deck panels, 560px (720px wide), 8px corners, Strong Hairline border, panel-lift shadow, rising 6px in 160ms. Actions sit right-aligned under a hairline: ghost Cancel, graphite alternatives, one Backlit Cap confirm.

### Scope Meters
The info panel's measures render as 20-segment meters (2px gaps, unlit segments #232428) that light in Pick, Star, Reject or neutral grey.

## Do's and Don'ts

### Do:
- **Do** keep every coloured pixel inside a lamp, a meter segment, a star, or the state text beside a lamp.
- **Do** give each surface exactly one Backlit Cap (#e3e5e9) for its primary action.
- **Do** print the keyboard shortcut on every key that has one, and drop the word before the legend when space runs out.
- **Do** put every count in a readout slot sized to its widest value, with tabular figures.
- **Do** mark the current photo with white corner crop marks.
- **Do** use 4px corners on hardware and 8px on modal panels.
- **Do** keep every rail on one line at the 960×640 minimum.
- **Do** animate keys with 1px travel in 60ms and lamps with a 120ms fade; respect reduced motion.

### Don't:
- **Don't** introduce a brand or accent hue; there is none.
- **Don't** tint a surface, fill a panel, or colour a border anywhere near a photo.
- **Don't** mark the current photo with a coloured border or glow.
- **Don't** light a second cap on the same surface; secondary actions are graphite.
- **Don't** show an unlit lamp as an outline or a missing dot; it is a dark filled lens.
- **Don't** use pill shapes or large radii on controls.
- **Don't** let digits reflow a rail as counts change.
