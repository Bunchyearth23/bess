# Integrated driving panel — 0.5.1

In the user's sketch, the floating driving window was crossed out and moved to an upper-right block. The interface follows that arrangement: listening bench on the left, driving on the right, full-width audio graph beneath both, then exports. Sound settings remain in the side panel.

The driving block appears at startup, and the top button shows or hides it. Its own scrolling reaches advanced settings without covering the rest. When central width is below 740 points, code stacks the blocks vertically. That narrow fallback was not exercised in a resized window during this delivery.

Delivery: `dist/BESS-0.5.1-dock.exe`; the earlier executable remains available. Driving controls, rpm bounds, DSP and WAV rendering remain, while layout and telemetry presentation change.

Release build, formatting and strict linting passed. A native capture with Genesis Phantom imported was inspected; it is excluded from the public package because the historic interface was French. No new DSP campaign was run for this UI-only change; audio evidence remained that of 0.5.
