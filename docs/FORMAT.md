# Inkstone notebook format v2

`.inkstone` is UTF-8 JSON. It is deliberately uncompressed and uses stable, semantic field names so
other programs and AI pipelines can parse it without running Inkstone. The current JSON Schema is
[`schema/inkstone-v2.schema.json`](../schema/inkstone-v2.schema.json). Version 1 single-canvas
documents are migrated on open.

Top-level fields:

```json
{
  "format": "inkstone.notebook",
  "version": 2,
  "title": "Motor project",
  "pages": [{
    "id": "d3f6e3d4-249c-4e75-9634-fef6404fca55",
    "title": "Motor control sketch",
    "canvas": {
      "background": { "red": 0.98, "green": 0.98, "blue": 0.97, "alpha": 1.0 },
      "grid_spacing": 24.0,
      "grid_visible": true
    },
    "layers": [{
      "id": "3de3a79c-b0ab-4b26-93aa-40fcfc4ad03a",
      "name": "Notes",
      "visible": true,
      "locked": false,
      "elements": []
    }]
  }],
  "assets": []
}
```

Coordinates and sizes are canvas-space floating-point values. Canvas space has no fixed bounds;
positive X points right and positive Y points down. Colors use unpremultiplied channels in `[0, 1]`.
Every element has a UUID `id` and a `type` discriminator.

## Layer types

A page is a stack of typed layers. Missing `kind` defaults to `notes`, so version 2 notebooks without
the field keep loading.

- `notes` (default): the original drawing layer. It stores `elements` (strokes, text, shapes,
  connectors, media).
- `excel`: a spreadsheet workbook that lives on the canvas. New sheets open as a 10×10 grid
  (`visible_cols` / `visible_rows`) that grows when you type past the edge, drag the resize handle,
  or use +Col/+Row. `elements` is empty; the workbook is in `spreadsheet`. Cells are sparse A1 keys (`"B12"`) with the typed input (`10`, `Revenue`,
  `=SUM(A1:A3)`) and optional style. Formulas use Excel A1 references, `$` anchors, sheet names
  (`Costs!A1`, `'My Sheet'!B2`), ranges, and a large Excel-compatible function set (`SUM`, `IF`,
  `VLOOKUP`, `INDEX`, `MATCH`, `COUNTIF`, date serials, and so on). Computed values are recalculated
  on load rather than cached.

```json
{
  "id": "7c1f0a2e-2b1a-4d3c-9f0e-1a2b3c4d5e6f",
  "name": "Budget",
  "visible": true,
  "locked": false,
  "kind": "excel",
  "elements": [],
  "spreadsheet": {
    "origin": { "x": 48.0, "y": 36.0 },
    "active_sheet": 0,
    "sheets": [{
      "name": "Sheet1",
      "cells": {
        "A1": { "input": "10" },
        "B1": { "input": "=A1*2", "style": { "bold": true, "number_format": "#,##0.00" } }
      }
    }]
  }
}
```

## Elements

- `stroke`: ordered `{x, y, pressure}` samples, stroke kind, RGBA color, and base width.
- `text`: UTF-8 text, baseline origin, font size, color, and optional wrapping width.
- `shape`: a semantic `kind`, bounds, rotation, stroke/fill style, and label. Kinds cover generic,
  electrical, and mechanical primitives.
- `connector`: start/end points, orthogonal route points, style, label, and optional attachments.
- `media`: bounds, searchable alt text/caption, semantic `image`/`pdf` kind, and an `asset_id`.
  Imported PDFs are rasterized to image assets (one notebook page per PDF page, locked background
  plus a notes layer) when `pdftoppm` is available.

Assets are stored once in the top-level `assets` array with UUID, filename, MIME type, and standard
base64 bytes. This keeps element relationships small and lets multiple future media frames reference
one payload. Current imports are capped at 64 MiB.

`frozen_rows` / `frozen_cols` pin worksheet panes from the active cell (Excel freeze-panes). SVG and
PDF export draw the used cell range plus frozen bands, not the empty 10×10 padding of a new sheet.

An attachment is explicit:

```json
{
  "point": { "x": 120.0, "y": 80.0 },
  "attachment": {
    "element_id": "2533c61f-4149-4ad2-89b7-728d68f69af8",
    "anchor": "east"
  }
}
```

This preserves both rendered geometry and the relationship to another element. Anchors are
`north`, `east`, `south`, `west`, `center`, `start`, or `end`. Inkstone rejects dangling
attachments, duplicate IDs, non-finite geometry, invalid colors, and unsupported versions.

## Compatibility

Readers should check both `format` and `version`, preserve unknown data when possible, and treat
`.inkstone` as canonical. A future incompatible format will increment `version`. Inkstone reads
`inkstone.document` version 1 and migrates it to a one-page, one-layer version 2 notebook; saving
writes version 2.

SVG exports contain `data-inkstone-id`, element kind, and connector endpoint element IDs for
lightweight downstream inspection. SVG does not retain pressure samples or all attachment fields.
