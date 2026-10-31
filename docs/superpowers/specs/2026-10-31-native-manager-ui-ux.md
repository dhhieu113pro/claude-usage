# Native manager window - UI/UX spec (Hallmark app-screen run)

**Hallmark · v2.0.0**

- **Screen** · app-screen scope, three screens of one Fluent window (Profiles · MCP servers · Settings)
- **Theme** · Windows 11 Fluent by platform: Mica, Segoe UI Variable, system accent colour, fixed type scale 1.125–1.2; labels sentence case, body face; buttons/labels never a display face
- **Signature** · the usage ring, the widget's own geometry and colour ramp: outer 16 DIP / inner 10 DIP / 4 DIP stroke; colours are `main.rs::level_color` verbatim — green (0.30, 0.78, 0.45), orange (0.98, 0.58, 0.16), red (0.92, 0.33, 0.33); threshold breaks at 60% / 90%
- **Density** · cards with inline MCP tags; one primary button style (accent-filled) per page; destructive actions are plain red-text buttons, confirmed by dialog, never styled as primary
- **States** · every page renders four states: loading (skeleton rows in the shape of cards), empty ("No profiles yet." / "No MCP servers yet." plus the one action that fills it), error (inline warning banner/cards), full
- **Motion** · none; state swaps are instant (native behaviour); no reveal on load, no width/height or all transitions
- **Sweep** · sloplint does not apply (no HTML to render); the checks below are the builder's manual gates and go into the commit/manual-verify notes

## Floor rules mapped to the native window (all binding)

1. One accent: the system accent marks the primary action, the selected sidebar item, and nothing else. Rings keep the widget's ramp — no accent-coloured rings.
2. Native focus visuals (`UseSystemFocusVisuals`, the WinUI default) are never suppressed — every control reachable and visibly focused.
3. Sentence-case labels and button text; never tracked capitals or small caps.
4. No emoji as icons. Use system glyphs or none.
5. Confirmations: an inline error first; a modal `ContentDialog` confirms destructive actions ("Remove \"x\" from the widget? Its files are kept." / "Delete \"x\" and remove it from every profile?") — modals are never used to show a save error.
6. Hit targets ≥ 44 px per control; control heights on the compact app range (32–40 px).
7. A number on screen is a real reading only. A profile with no reading (widget error or first poll pending) shows an empty track ring with its label — no fabricated percent, no invented metric.
8. Tooltips carry whole values (long dirs/urls) — the card shows a clipped value, the tooltip the full one, as the HTML pages did with `title`.

## Screen 1 · Profiles

- Command bar (top of the page content): "Add existing…" (secondary) and "+ New profile" (primary, accent).
- Profile cards:
  - Left: two mini ring meters, session and weekly usage, 40 DIP each, labels "session" / "week" in the caption size; the weekly card carries the reset date under its ring ("resets Oct 29") when known.
  - Centre: profile name (strong), source line (`folder: …` / `command: …` / `default (~/.claude)`), resolved directory (caption, clipped+tooltip), MCP tags as small chips (or the muted "no MCP servers").
  - Right: "Edit" and "Remove" text buttons (Remove is the −danger red).
  - A profile with a `.claude.json`-read error renders its card with the error line and muted rings (empty tracks); its actions stay enabled except nothing claims data it cannot read.
- Empty state: "No profiles yet." with a single "New profile" button.
- Edit dialog (name, launcher command, config folder with a native Browse… button, sub-captions copyed verbatim from the HTML dialog) and New-profile dialog (name, MCP checkbox list or "No shared MCP servers yet.", sub-caption), both Save/Cancel with inline error text; after Create the dialog shows the multiline result (dir, run, notes) and Create becomes disabled.
- Remove confirm as above, then refresh.

## Screen 2 · MCP servers

- Command bar: "from <profile>" dropdown + "Import" (secondary), "+ Add server" (primary).
- Server cards: name (strong) + type chip ("stdio"/"http"), summary line (`command args…` or url, clipped+tooltip); under it the enable matrix: one labelled checkbox per profile; a profile with a read error shows a disabled checkbox whose tooltip is the error.
- Per-card "Delete" (red text, confirm dialog) and "Edit".
- Profile read errors listed as warning cards below the list, exactly as the page does now.
- Add/Edit dialog fields verbatim from the HTML dialog: Name, Type ("stdio (local command)" / "http (remote URL)"), Command, "Arguments (one per line)", env rows (NAME/value + remove × + "+ Add variable"), URL, header rows ("+ Add header"); Save/Cancel; dialog-internal errors inline.

## Screen 3 · Settings

- One card "Refresh": numeric field "Refresh every (seconds, minimum 15)" prefilled from a fresh read, sub-caption "How often usage is polled. The old numbers stay visible while a refresh runs.", primary Save, inline error / "Saved." acknowledgement; the rings visibly re-fetch on save.

## Signature rendering (builder-facing)

- `RingMeter` component in `manager.rs`: `windows-canvas = "0.100"` with the `reactor` feature, host mode `canvas_invalidated` + `Invalidator` (redraw only on data change — the ring is state, never decoration). The frame draws: a full neutral track circle (widget grey at reduced alpha), then the usage arc from 12 o'clock with `level_color(used)`, clamped 0..100, using main.rs's exact arc geometry. Each meter keeps its own `Invalidator`; a card redraws when its `ProfileState` changes.
- If the canvas host integration fights the page (layoutY/recovery issues), the fallback is two thin rounded bars (same `level_color` ramp, same clamp) with the decision recorded in the spec's "Builder check - answered" section — the signature bar heights are the same in the mock checks.

## States source of truth

- Usage readings come from the widget's live `STATE` mutex in `main.rs` (matched by profile name, last reading kept as it does); the manager never polls usage itself — it reads what the widget already fetched. `ManagerState::profiles` gains `session_used`, `weekly_used` (Option<f32>) and `weekly_reset` (Option<String>) sourced there.
