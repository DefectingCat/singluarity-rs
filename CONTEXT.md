# Context

## Camera auto-orbit decision

The camera opens on a stationary close view just below the disk. The Camera panel can start a ten-second flyby from distance 44 to the default 28.5, or enable a continuous 45-second orbit. Dragging or zooming the scene interrupts the flyby. Manual input and active UI controls pause continuous orbit, which resumes after 1.5 seconds of inactivity. The field of view follows the shorter viewport axis so portrait windows keep the main lensed arcs in frame.

## egui slider writeback gotcha (camera sliders must use `SliderClamping::Edits`)

Widgets bound to state that other systems also write (the live `OrbitCamera` resource) must NOT use egui's default `SliderClamping::Always`: `Slider::add_contents` then calls `set_value(get_value())` on every idle frame, and `set_value` rounds to `fixed_decimals`, silently snapping the live value to the slider's decimal grid each frame. This pinned auto-orbit (per-frame yaw step ~0.0023 rad < the 0.005 half-quantum of a 2-decimal grid) and presented as "background stars shake, no rotation". With `Edits`, clamping/rounding applies only to real user edits; idle frames never write back. Guarded by `idle_camera_panel_does_not_mutate_camera` in `src/ui/sections.rs`.
