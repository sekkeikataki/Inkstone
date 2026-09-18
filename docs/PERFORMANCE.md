# Performance architecture

Inkstone uses an event-driven GTK4/libadwaita process and a retained Rust vector document. It does
not run a web engine, background renderer, animation loop, network service, or idle poller.

## Current choices

- Stroke samples and geometry use `f32`, halving point storage versus `f64`; conversion to Cairo's
  `f64` happens only while drawing.
- Input samples closer than 0.7 screen pixels are coalesced. This limits memory and rendering work
  without reducing visible detail at the current zoom.
- Drawing is queued only after input, edits, or view changes. A static note consumes no continuous
  render CPU.
- Sidebar, toast, and tool-option transitions use libadwaita's native animation machinery and short
  GTK CSS fades. Keyboard zoom uses a GTK frame-clock tick callback for a 180 ms ease-out, then
  stops; superseded zooms cancel the previous callback. Page switches and new selections use the
  same short tick (page fade / selection flash) and then stop. Drawing hides chrome via GtkRevealer
  and AdwToolbarView, which unreveal after the stroke ends. Reduced-motion desktops skip these
  eases. No idle animation loop runs while the notebook is still.
- Autosave uses a generation-based, one-shot two-second debounce after edits and only activates
  after a destination has been chosen. It does not poll, and stale timers exit without writing.
- Elements outside the expanded viewport are rejected by a cheap bounds test before Cairo work.
  Grid spacing doubles at low zoom, bounding line count and avoiding sub-pixel overdraw.
- Pressure is rendered directly from the stored sample vector. Consecutive sample pairs whose
  widths differ by at most 0.08 canvas units share one Cairo stroke, so ordinary handwriting is
  a few path batches instead of one stroke per segment. There is no full-canvas bitmap, so the
  canvas remains effectively unbounded and memory follows document complexity rather than zoom
  or canvas extent.
- Spreadsheet grids are one vertical and one horizontal line pass; empty cells are skipped. Frozen
  pane overlays redraw only when the viewport has scrolled past the freeze origin. Column and row
  headers cull to the visible clip. Stroke erase batches one history entry per drag and ignores
  samples closer than 0.4 of the eraser radius.
- Undo history is operation-based and capped at 256 edits. Added objects are moved into redo storage
  only when undone rather than permanently duplicated. Selection transforms retain copies only of
  affected objects and attached connectors, not the page or notebook.
- Image bytes are stored once as notebook assets and decoded pixbufs are cached by asset UUID.
  Viewport rendering reuses them instead of decoding per frame.
- Save uses compact typed Rust values and an atomic same-directory temporary-file rename. No
  document copy is retained after save.
- Release builds use thin LTO, one codegen unit, and stripped symbols.

## Complexity and limits

Rendering is `O(n + v)` per damaged frame: all `n` element bounds are checked and visible geometry
`v` is drawn. This has low fixed memory and performs well for normal sparse notes. Very dense
documents (roughly 10,000+ objects, depending on stroke length and hardware) need a tile/R-tree
spatial index and cached tessellation; those are intentionally deferred until profiling can justify
their memory and invalidation cost.

Cairo pressure rendering batches consecutive similar-width segments. Rapid pressure changes still
emit shorter strokes; extreme imported polylines would still benefit from tessellated variable-width
geometry if profiling shows the remaining path count is a bottleneck.

## Verification

The required release check is:

```bash
cargo build --release
```

Model tests cover round trips, relationship validation, snapping, viewport bounds, and semantic SVG
metadata. Runtime tablet latency and compositor-specific power draw require physical Linux hardware
and are not represented by unit tests.
