# Inkstone

Inkstone is a native Linux infinite-canvas notebook for handwriting, typed notes, and early
electrical/mechanical sketches. It is written in Rust with GTK4/libadwaita: there is no browser
runtime, Electron layer, or network service.

## What works

- Infinite pan/zoom canvas with an adaptive grid
- Mouse and single-touch drawing; middle-button or Pan-tool panning; touchpad scrolling
- Two-finger pinch zoom and Ctrl+scroll zoom around the pointer
- GTK tablet/stylus input with pressure-width rendering and automatic eraser-tip detection
- Multi-page notebooks with sections, subpages, a recycle bin, independent layered canvases,
  visibility, locking, background color, patterns (plain/grid/dots/lined), ISO 216 paper sizes
  (A5–A2) or an infinite canvas, and millimetre grid spacing
- A local notebook library in `Documents/Inkstone`: folders are categories (nest freely), each
  `.inkstone` file is a notebook. The sidebar lists the same layout your file manager shows.
- Insert menu for tables, OneNote-style tags, ISO 8601 date/time, file/audio attachments, page templates,
  and local equation calculate (`2+2=`)
- Typed notes with bold/italic/underline, bullets/numbered/to-do lists, and typewriter size.
  Click existing text with the Text tool to keep editing it
- Ink to shape, a ruler for horizontal/vertical ink, and Shift for 15° ISO angles or square shapes
- Measure tool that drops ISO 129 millimetre dimensions
- Stroke widths in millimetres (ISO 128: 0.25–1.4 mm chips, 0.13–70 mm range) with custom chips
- Responsive libadwaita workspace: clickable, in-place-renamable page/layer lists, a grouped
  left tool rail with contextual color/width options, live zoom, and a Tools button that hides
  the header and canvas chrome without auto-hiding while you draw
- Typed notes with adjustable typewriter size, plus cross-page text/label search
- Selection with lasso, move, corner resize, rotate handle, z-order, cut/copy/paste, duplicate,
  and delete
- Pen, brush, highlighter (drawn behind ink), dashed strokes, translucent shape fill, and a
  stroke-splitting eraser
- Vertical-space tool to insert or collapse room on a page
- Geometry plus IEC 60617 symbols (including lamp and transformer), IEC logic gates, and ISO 128/129
  surface-finish and third-angle symbols; lines and arrows keep drag direction
- Orthogonal connectors that snap to semantic anchors and retain those relationships
- Drag-and-drop plus menu import for PNG/JPEG/WebP/GIF images, SVG drawings, PDF as inkable pages,
  audio, and other file attachments
- Undo/redo, debounced autosave after the first manual save, protected unsaved close/new/open,
  native file dialogs, printing, SVG/PDF/PNG/JPEG export, and folder export of pages/layers
- Library-wide search, to-do rollup, page thumbnails, optional open-notebook tabs, night paper,
  Cornell/lab/ISO title-block templates, editable tables, `[[Page]]` links, and ink replay that
  redraws the current page in stroke order
- A Settings dialog (Ctrl+,) covering startup, appearance, ink feel, drawing defaults, pages,
  text, zoom, and the local library folder
- Canonical `.inkstone` JSON with strokes, tables, pressure points, shapes, labels, connector routes,
  and attached element/anchor IDs

## Arch Linux

Install build/runtime dependencies:

```bash
sudo pacman -S --needed rustup gtk4 libadwaita pkgconf xdg-desktop-portal-gtk
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

## Controls

Choose a tool from the floating palette. Pen, Shape, Connector, and Measure use click-drag. Hold
Shift to snap ink and lines to 15° or to keep rectangles/ellipses square. Escape cancels a drag.
Text places the contents of the note field at the clicked position, or edits a note you click.
Shape/connector labels and ink options appear in the chip above the canvas. Click a page or layer
name in the sidebar to rename it.

| Action | Input |
| --- | --- |
| Draw/use active tool | Primary mouse, single touch, or stylus |
| Tools | V select, P pen, H highlighter, E eraser, T text, S shape, M measure (when the canvas has focus) |
| Pan | Middle-drag, Pan tool, or two-axis scroll |
| Zoom | Pinch, Ctrl+scroll, Ctrl+Plus/Minus, or the zoom chip |
| Tablet erase | Flip to an eraser tip when GTK reports one |
| Select, move, resize, rotate | Select tool: drag objects, drag corner handles, or the top rotate handle |
| Cut/copy/paste | Ctrl+X / Ctrl+C / Ctrl+V |
| Duplicate/delete selection | Ctrl+D / Delete |
| Bring to front / send back | Ctrl+] / Ctrl+[ |
| Insert vertical space | Space tool, then drag up or down |
| Measure millimetres | Measure tool, then drag |
| Constrain to 15° / square | Hold Shift while drawing or shaping |
| New notebook | Ctrl+N, or + next to Notebooks |
| New category folder | Folder+ next to Notebooks |
| Show library in Files | Folder button next to Notebooks |
| Search | Ctrl+F, then Enter for the next match |
| Import image/PDF/SVG | Ctrl+I, drag onto the canvas, or File → Import |
| Print | Ctrl+P |
| Save/open | Ctrl+S / Ctrl+O |
| Undo/redo | Ctrl+Z / Ctrl+Shift+Z |
| Settings | Ctrl+, or the gear in the header |
| Previous/next page | Alt+Left / Alt+Right |
| Show/hide notebook sidebar | Notebook button, F9, or the main menu |
| Show/hide drawing tools | Tools button, F10, or the main menu |
| Show/hide colors or widths | Colors/Widths buttons in the header, the restore chips, or the View menu |
| Fullscreen | F11 |
| Insert table, tag, date, file | Insert menu in the header |
| Calculate `2+2=` | Type an equation ending with `=` and click, or Insert → Calculate |

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

- No on-device handwriting recognition, cloud sync, Microsoft account features, collaboration,
  plugin marketplace, or `.xopp` interchange. Ink-to-text is omitted unless a local engine exists.
- PDF pages rasterize with `pdftoppm` or Ghostscript when those tools are installed; otherwise Inkstone
  still creates an inkable sheet with an embedded PDF card.
- Audio while inking uses `pw-record`, `parecord`, or `arecord` when present, and playback via the
  desktop opener. There is no bundled GStreamer pipeline.
- Palm rejection ignores touch after recent stylus activity. Tilt and barrel-button mapping depend on
  what GTK reports for the device.
- SVG keeps visible content and connector element IDs, but per-point pressure and full semantic
  attachment details remain authoritative only in `.inkstone`.
- Rendering scans element bounds before drawing. Viewport culling keeps sparse documents cheap,
  but a spatial index will be needed for very dense documents above roughly 10,000 objects.
- Version 1 documents and version 2 notebooks migrate to the current version 3 format on open.

Inkstone is independent software and does not use third-party notebook branding or assets.
