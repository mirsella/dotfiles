---
name: demo
description: Record a short video demo of session work in a web app, desktop app, mobile app, or game.
---

Record a quick demo of this session's work in the actual app.
Keep it under 30 seconds, at native 2K with constant 60 fps rendering and recording,
with a visible nice cursor and click highlights.
Capture at 60 fps directly, not by converting a lower-frame-rate recording.
Verify that actual rendering sustains 60 fps throughout the take; encoder metadata
alone is not proof. Don't use interpolation or duplicated frames to conceal
rendering drops, or silently lower the resolution or frame-rate requirement.
Use natural UI interactions where it makes sense to show UI stuff.
Only use injected state and programmatic navigation where it makes senses, or to triggers events.
Cut idle time, verify the output resolution and frame rate, and return the video.
