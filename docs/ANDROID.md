# Android companion

The Linux GTK app stays the desktop editor. Android opens the same `.inkstone` v2 JSON so you can
view a page, add ink or a text note, and edit spreadsheet cells on a phone.

## What it does

- Open an existing notebook from Files, Drive, or a share/`VIEW` intent
- Create a new blank notebook
- Pan/zoom the infinite canvas (strokes, text, shapes, connectors, images, worksheet objects)
- Quick ink and text on the first unlocked notes layer
- Cell grid for spreadsheet layers; edits write A1 `input` values the desktop engine recalculates
- Save writes pretty-printed `inkstone.notebook` version 2

It does not run GTK, VBA, charts, or the full desktop tool rail. Unknown JSON fields are kept in
the document tree so a phone save does not strip desktop-only data.

## Build an APK

Install Android Studio (or the command-line SDK: platform 35, build-tools 35, and a JDK 17+).

```bash
cd android
./gradlew assembleDebug
```

The debug APK is `android/app/build/outputs/apk/debug/app-debug.apk`. Sideload it, then open
[`examples/phone-demo.inkstone`](../examples/phone-demo.inkstone) from the device.

Release signing is left to your keystore:

```bash
./gradlew assembleRelease
```

## Shared format

The companion parses the same document the Rust core validates. `cargo test --no-default-features`
runs that core without GTK. The desktop crate still defaults to the `desktop` feature.
