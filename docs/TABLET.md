# Linux tablet and Lenovo stylus notes

Inkstone consumes GTK4 `GestureStylus` events. Pressure comes from GDK's `Pressure` axis, and a
physical eraser is selected when GDK identifies the device tool as `Eraser`. Mouse and touch use a
separate GTK gesture path with pressure `1.0`.

On Arch, use a Wayland GNOME session where practical and keep the firmware stack current:

```bash
sudo pacman -S --needed libinput libwacom fwupd
libinput list-devices
```

Lenovo Yoga/ThinkPad pen devices are commonly exposed through the kernel HID/Wacom drivers and
libinput without application-specific code. Verify that `libinput list-devices` reports the tablet
and that pressure/eraser work in another GTK drawing application. Inkstone receives only axes that
GTK/GDK exposes.

Current behavior:

- Pen tip uses the selected canvas tool and pressure-modulated width.
- A reported physical eraser temporarily overrides the selected tool.
- Single touch operates the selected tool; two-finger pinch changes zoom.
- Middle mouse or the Pan tool pans. Touchpad scroll pans; Ctrl+scroll zooms.

Palm rejection, pen-button mapping, calibration, and rotation are compositor/libinput concerns in
this version. X11 support depends on the same GTK/GDK exposure but is less consistent across tablet
models. Hardware verification is required for any specific Lenovo model because firmware and
digitizer vendors differ within product families.
