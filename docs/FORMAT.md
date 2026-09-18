# Inkstone notebook format v3

`.inkstone` is UTF-8 JSON. It is deliberately uncompressed and uses stable, semantic field names so
other programs and AI pipelines can parse it without running Inkstone. The current JSON Schema is
[`schema/inkstone-v3.schema.json`](../schema/inkstone-v3.schema.json). Version 1 single-canvas
documents and version 2 notebooks are migrated on open.

Top-level fields:

```json
{
  "format": "inkstone.notebook",
  "version": 3,
  "title": "Motor project",
  "sections": [
    {"id": "11111111-1111-1111-1111-111111111111", "name": "Notes", "color": {"red": 0.12, "green": 0.38, "blue": 0.88, "alpha": 1.0}},
    {"id": "22222222-2222-2222-2222-222222222222", "name": "Quick Notes", "color": {"red": 0.95, "green": 0.62, "blue": 0.05, "alpha": 1.0}}
  ],
  "active_section": "11111111-1111-1111-1111-111111111111",
  "pages": [{
    "id": "d3f6e3d4-249c-4e75-9634-fef6404fca55",
    "title": "Motor control sketch",
    "section_id": "11111111-1111-1111-1111-111111111111",
    "level": 0,
    "canvas": {
      "background": { "red": 0.98, "green": 0.98, "blue": 0.97, "alpha": 1.0 },
      "grid_spacing": 14.173,
      "grid_visible": true,
      "pattern": "grid",
      "layout": "infinite",
      "page_width": 595.276,
      "page_height": 841.89
    },
    "layers": [{
      "id": "3de3a79c-b0ab-4b26-93aa-40fcfc4ad03a",
      "name": "Notes",
      "visible": true,
      "locked": false,
      "elements": []
    }]
  }],
  "assets": [],
  "trash": []
}
```

Coordinates and sizes are canvas-space floating-point values in PostScript points
(72 pt = 25.4 mm, ISO 216). New notebooks default to a 5 mm grid and A4 sheet size
(210×297 mm). Stroke widths follow ISO 128 line groups and are edited in millimetres.
Positive X points right and positive Y points down. Colors use unpremultiplied channels in `[0, 1]`.
Every element has a UUID `id` and a `type` discriminator. Pages belong to a section (`section_id`)
and may be indented as subpages (`level` 1). Deleted pages move to `trash` until the recycle bin is
emptied.

## Elements

- `stroke`: ordered `{x, y, pressure}` samples, stroke kind (`pen`, `highlighter`, `brush`), RGBA
  color, base width, and optional dashed flag. Ink replay uses this sample order and path length;
  timestamps are not stored.
- `text`: UTF-8 text, baseline origin, font size, color, optional wrapping width, plus local rich-text
  flags (`bold`, `italic`, `underline`, `highlight`, `list`, `href`, `checked`).
- `shape`: a semantic `kind`, bounds, rotation, stroke/fill style, and label. Kinds cover generic
  geometry (rectangle, ellipse, line, arrow, triangle, dimension) plus IEC 60617 electrical
  and mechanical primitives (resistor rectangle, capacitor, diode, inductor, switch, fuse,
  battery, earth, lamp, transformer, motor, gear, bearing, spring, beam) plus IEC logic gates
  and ISO 128/129 surface-finish and third-angle symbols. Line, arrow, and dimension bounds keep
  signed width/height so drag direction is preserved.
- `connector`: start/end points, orthogonal route points, style, label, and optional attachments.
- `media`: bounds, searchable alt text/caption, semantic `image`/`pdf`/`file`/`audio` kind, and an
  `asset_id`.
- `table`: bounds, `columns`, `rows`, and a row-major `cells` string array.
- `tag`: origin, kind (`to_do`, `important`, `question`, `idea`, `critical`, `definition`,
  `contact`, `address`, `phone`, `date`), note, and optional checkbox state.

Assets are stored once in the top-level `assets` array with UUID, filename, MIME type, and standard
base64 bytes. This keeps element relationships small and lets multiple media frames reference one
payload. Current imports are capped at 64 MiB.

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
`inkstone.document` version 1 and version 2 notebooks, then migrates them to a sectioned version 3
notebook with Notes / Quick Notes defaults; saving writes version 3.

Microsoft-account, OneDrive, Copilot, Teams, Outlook, and other online-only OneNote services are
intentionally out of scope. Inkstone stays local-first.

Notebooks live as ordinary `.inkstone` files under `Documents/Inkstone` (override with
`INKSTONE_LIBRARY`). Each folder there is a category; nested folders are allowed. Sections and
pages stay inside the notebook file so the file manager never duplicates that tree.

Window preferences (theme, ink feel, drawing defaults, new-page options, zoom, export, last
notebook, library folder) live in `~/.config/inkstone/session.json`, or
`$XDG_CONFIG_HOME/inkstone/session.json`. They are not part of the notebook format.
`INKSTONE_LIBRARY` still overrides a saved library folder.

SVG exports contain `data-inkstone-id`, element kind, and connector endpoint element IDs for
lightweight downstream inspection. SVG does not retain pressure samples or all attachment fields.
