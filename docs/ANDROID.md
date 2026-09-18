# Android companion

The Linux GTK app stays the desktop editor. Android opens the same `.inkstone` v2 JSON so you can
view a page, add ink or a text note, and edit spreadsheet cells on a phone.

## What it does

- Open an existing notebook from Files, Drive, or a share/`VIEW` intent
- Create a new blank notebook
- Pan/zoom the infinite canvas (strokes, text, shapes, connectors, images, worksheet objects)
- A bottom tool bar: **Pan**, **Draw**, **Erase**, **Text**, **Cells**, **+ Page**, **+ Sheet**
- Draw, erase, and tap-to-edit text on a notes layer (created automatically if needed)
- Cell grid for spreadsheet layers; **+ Sheet** adds one; edits write A1 `input` values
- **Save** writes pretty-printed `inkstone.notebook` version 2 back to the opened file

It does not run GTK, VBA, charts, or the full desktop tool rail. Unknown JSON fields are kept in
the document tree so a phone save does not strip desktop-only data.

## Build an APK

Install Android Studio (or the command-line SDK: platform 35, build-tools 35, and a JDK 17+).

```bash
cd android
./gradlew assembleDebug
```

A sideloadable debug APK is checked in at
[`releases/inkstone-android.apk`](../releases/inkstone-android.apk). Download that file onto the
phone, enable install from this source, and open [`examples/phone-demo.inkstone`](../examples/phone-demo.inkstone).
Rebuilding locally still writes `android/app/build/outputs/apk/debug/app-debug.apk`.

Release signing is left to your keystore:

```bash
./gradlew assembleRelease
```

## Shared format

The companion parses the same document the Rust core validates. `cargo test --no-default-features`
runs that core without GTK. The desktop crate still defaults to the `desktop` feature.
