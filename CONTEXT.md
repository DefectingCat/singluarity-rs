# Context

## Camera auto-orbit decision

The camera auto-rotates by increasing yaw only when enabled by default. It matches a rightward mouse drag, completes one revolution in about 45 seconds, and starts immediately. Left-button dragging, wheel zoom, and active UI control manipulation pause the rotation; it resumes after 1.5 seconds of no interaction. Mere pointer hover does not pause it. The Camera panel exposes an on/off toggle; speed is fixed and not user-adjustable.
