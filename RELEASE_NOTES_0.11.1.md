# BESS 0.11.1

Current source revision: one physical engine powers scratch engines and Automation resynthesis. A retains original source playback; B and all sound exports use the physical engine. Automation metadata configures the engine with visible assumptions, and 19 physical sound controls are saved with projects. Listening uses a 3 ms lookahead peak limiter.

Camera perspective previews, idle gain, file level estimates and interface ergonomics remain available.

## New features & improvements

- **BeamNG camera perspective preview**:
  - Live in-bench acoustic emulation of BeamNG camera views: **Cockpit / Interior**, **Hood**, **Tailpipe**, and **Orbit / Exterior**.
  - Cockpit view applies in-cabin low-pass filtering (~1100 Hz cutoff) and firewall attenuation with dominant engine presence and attenuated exhaust.
  - Hood view emphasizes direct intake and engine bay acoustic radiation with distant exhaust.
  - Tailpipe view delivers full-strength exhaust output with distant engine presence.
  - Smooth 50 ms slew crossfade when switching perspectives during real-time playback.
  - Live peak meter in dBFS for the selected camera perspective.
- **Dedicated idle gain control**:
  - New `Idle gain` slider (range 0.0 to 2.0, default 1.0) with a smooth cubic Hermite transition between idle RPM and ~2200 RPM.
  - Scaled consistently across physical listening, WAV export and BeamNG volume estimation without affecting higher RPM levels.
  - Available directly in primary sound controls, advanced controls, and the BeamNG volume estimation box.
- **BeamNG camera volume report**:
  - Level analysis panel now includes a camera volume preview table with estimated in-game dBFS levels and engine/exhaust balance for off-load and full-load states across all camera perspectives.
- **UI and accessibility**:
  - Camera preview buttons are permanently visible in the sound comparison bench, allowing instant one-click switching to the BeamNG two-emitter preview.
  - Entire user interface and documentation unified in English.

## Windows downloads

- `BESS-0.11.1-Windows-Portable.zip`: main application, start page, guide, release notes and file hashes.
- `BESS-0.11.1-Windows.exe`: main application without the portable folder.
- Each archive and executable has a matching `.sha256` sidecar.

The packages contain no test vehicles, Automation sound banks, prepared BeamNG add-ons or listening corpus. Import your own Automation vehicle ZIP into the main application. Keep the original vehicle ZIP beside a new BESS configuration add-on and remove an older add-on for the same vehicle before enabling it.

The source checkout requires the pinned private BDSP dependency to build, while the Windows binaries run without repository access. Automated tests, spectral checks and package hashes do not establish how a vehicle sounds inside BeamNG; in-game listening remains the final check.
