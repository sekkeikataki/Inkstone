# Inkstone

Inkstone is a native Linux infinite-canvas notebook for handwriting, typed notes, and early
electrical/mechanical sketches. It is written in Rust with GTK4/libadwaita: there is no browser
runtime, Electron layer, or network service. An Android companion in `android/` opens the same
`.inkstone` files for viewing and quick edits on a phone.

## What works

- Infinite pan/zoom canvas with an adaptive grid
- Mouse and single-touch drawing; middle-button or Pan-tool panning; touchpad scrolling
- Two-finger pinch zoom and Ctrl+scroll zoom around the pointer
- GTK tablet/stylus input with pressure-width rendering and automatic eraser-tip detection
- Multi-page notebooks with independent layered canvases, visibility, locking, grid controls, and
  typed layers (notes or Excel-compatible spreadsheets)
- Responsive libadwaita workspace: clickable, in-place-renamable page/layer lists, a grouped
  left tool rail with contextual color/width options, live zoom, canvas-first chrome that slides
  away while drawing, and short page/selection/empty-state motion
- Typed note placement and cross-page text/label/spreadsheet search
- Selection, lasso selection, drag-to-move, resize/rotate handles, duplicate, delete, and
  relationship-aware connectors
- Pen colors, pressure ink, translucent highlighter, and a stroke-splitting eraser
- Rectangle, ellipse, resistor, capacitor, ground, motor, gear, bearing, spring, and beam symbols
- Orthogonal connectors that snap to semantic anchors and retain those relationships
- Embedded PNG/JPEG/WebP/GIF images and PDF pages rasterized onto locked, annotatable canvas layers
- Undo/redo, debounced autosave after the first manual save, protected unsaved close/new/open,
  native file dialogs, SVG page export, and multi-page PDF export
- System clipboard: worksheet TSV/HTML copy, and sketch copy as Inkstone JSON plus a PNG snapshot
- Open, pretty-printed `.inkstone` JSON with UUIDs, text, pressure points, shapes, labels, connector
  routes, and attached element/anchor IDs

## Arch Linux

Install build/runtime dependencies:

```bash
sudo pacman -S --needed rustup gtk4 libadwaita pkgconf xdg-desktop-portal-gtk poppler
rustup toolchain install 1.98.1
```

Run:

```bash
cargo run --release
```

Install for the current user:

```bash
make install PREFIX="$HOME/.local"
```

`~/.local/bin` must be on `PATH`. To remove it, run
`make uninstall PREFIX="$HOME/.local"`.

## Android

The phone app is a Jetpack Compose companion that reads and writes the same notebook JSON. Build it
with Android Studio or:

```bash
cd android
./gradlew assembleDebug
```

Download [`releases/inkstone-android.apk`](releases/inkstone-android.apk) and sideload it, then open
a `.inkstone` file (try [`examples/phone-demo.inkstone`](examples/phone-demo.inkstone)). Use **Page**
to pan/zoom, ink, or tap-add text; use **Cells** to edit a worksheet. Details are in
[`docs/ANDROID.md`](docs/ANDROID.md).

## Controls

Choose a tool from the floating palette. Pen, Shape, and Connector use click-drag. Text places the
contents of the note field at the clicked position. Shape/connector labels and ink options appear in
the chip above the canvas. Click a page or layer name in the sidebar to rename it. The layer add
menu can insert a notes layer or a spreadsheet layer. Spreadsheet layers spawn as a 10×10
workbook on the canvas: click cells, type values or `=` formulas, use the name box and formula bar,
Enter/Tab to grow past the edge, drag any corner handle to resize the grid, +Col/+Row to add
blocks of cells, Freeze to pin rows/columns from the active cell, and drag the green title bar to
reposition. Column and row headers select, resize, and fill like a worksheet. Copy places TSV and
HTML on the system clipboard; print and SVG/PDF export use the used cell range instead of empty
padding.

| Action | Input |
| --- | --- |
| Draw/use active tool | Primary mouse, single touch, or stylus |
| Pan | Middle-drag, Pan tool, or two-axis scroll |
| Zoom | Pinch, Ctrl+scroll, Ctrl+Plus/Minus, or the zoom chip |
| Tablet erase | Flip to an eraser tip when GTK reports one |
| Select/lasso and move | Select tool, then click/drag or drag an empty region |
| Duplicate/delete selection | Ctrl+D / Delete (on a sheet: fill down / clear cells) |
| Copy/cut/paste | Ctrl+C / Ctrl+X / Ctrl+V (sheet TSV/HTML or sketch image) |
| Search | Ctrl+F, then Enter for the next match |
| Import image/PDF | Ctrl+I, or File → Import Image or PDF |
| Save/open | Ctrl+S / Ctrl+O |
| Undo/redo | Ctrl+Z / Ctrl+Shift+Z |
| Reset view | Ctrl+0 |
| Previous/next page | Alt+Left / Alt+Right |
| Show/hide notebook sidebar | Notebook button, F9, or the main menu |

The `.inkstone` document is canonical. SVG is a portable visual export. See
[`docs/FORMAT.md`](docs/FORMAT.md), [`docs/PERFORMANCE.md`](docs/PERFORMANCE.md), and
[`docs/TABLET.md`](docs/TABLET.md).

## Development

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

## First-version limitations

- No rich-text spans, handwriting recognition, audio recording, collaboration, or cloud sync yet.
  Spreadsheet layers cover Excel-compatible formulas, formatting, sheets, fill, freeze panes, and
  sort, but not pivot tables, charts, or VBA.
- Palm rejection and device calibration are delegated to GTK/GDK, libinput, and the compositor.
- Native file dialogs use the desktop FileChooser portal when one is advertised. Minimal window
  manager sessions must run a working portal backend such as `xdg-desktop-portal-gtk`.
- SVG keeps visible content and connector element IDs, but per-point pressure and full semantic
  attachment details remain authoritative only in `.inkstone`.
- Rendering scans element bounds before drawing. Viewport culling keeps sparse documents cheap,
  but a spatial index will be needed for very dense documents above roughly 10,000 objects.
- Version 1 documents are validated strictly; there is no migration path for future versions yet.
- PDF annotation rasterizes each page with `pdftoppm` (Poppler). If Poppler is missing, Inkstone
  still embeds a portable PDF attachment card.

Inkstone is independent software and does not use third-party notebook branding or assets.
