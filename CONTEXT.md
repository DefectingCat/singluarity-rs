# Context

## Camera auto-orbit decision

The camera auto-rotates by increasing yaw only when enabled by default. It matches a rightward mouse drag, completes one revolution in about 45 seconds, and starts immediately. Left-button dragging, wheel zoom, and active UI control manipulation pause the rotation; it resumes after 1.5 seconds of no interaction. Mere pointer hover does not pause it. The Camera panel exposes an on/off toggle; speed is fixed and not user-adjustable.

## egui slider writeback gotcha (camera sliders must use `SliderClamping::Edits`)

Widgets bound to state that other systems also write (the live `OrbitCamera` resource) must NOT use egui's default `SliderClamping::Always`: `Slider::add_contents` then calls `set_value(get_value())` on every idle frame, and `set_value` rounds to `fixed_decimals`, silently snapping the live value to the slider's decimal grid each frame. This pinned auto-orbit (per-frame yaw step ~0.0023 rad < the 0.005 half-quantum of a 2-decimal grid) and presented as "background stars shake, no rotation". With `Edits`, clamping/rounding applies only to real user edits; idle frames never write back. Guarded by `idle_camera_panel_does_not_mutate_camera` in `src/ui/sections.rs`.
