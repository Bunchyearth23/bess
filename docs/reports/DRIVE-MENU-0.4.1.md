# Separate driving window — 0.4.1

The top-bar **Driving…** button opened a movable, closable window with simulated/direct/cycle modes, throttle, gearbox, load and bench settings. Direct rpm/load controls moved there too, along with play/pause and compact telemetry.

The left panel became **Sound settings**, holding source vehicle, character, exhaust, intake/mechanical sound and saving, without driving controls.

This was a UI-only change. Release build, strict linting and formatting passed; no new DSP tests. An internal capture was inspected with the window open and ZIP loaded. Older versions remained available. The new window normally started closed; `--capture-driving image.png vehicle.zip` captured it open for validation. The historic French-interface capture is excluded from the public package.

Executable: `dist/BESS-0.4.1.exe`. SHA256: `E7163F4F8BFFA915B9EE3F4BB52D475A64A44480C6800280A7544D8F1B017EC2`.
