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

## Elements

- `stroke`: ordered `{x, y, pressure}` samples, stroke kind, RGBA color, and base width.
- `text`: UTF-8 text, baseline origin, font size, color, and optional wrapping width.
- `shape`: a semantic `kind`, bounds, rotation, stroke/fill style, and label. Kinds cover generic,
  electrical, and mechanical primitives.
- `connector`: start/end points, orthogonal route points, style, label, and optional attachments.
- `media`: bounds, searchable alt text/caption, semantic `image`/`pdf` kind, and an `asset_id`.

Assets are stored once in the top-level `assets` array with UUID, filename, MIME type, and standard
base64 bytes. This keeps element relationships small and lets multiple future media frames reference
one payload. Current imports are capped at 64 MiB.

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
