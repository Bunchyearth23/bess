# Labelled BeamNG vehicle installation — 2026-09-23

The twelve Automation vehicle archives in `cars` were exported again with the current BESS source. Each exported vehicle's top-level `vehicles/<model>/info.json` `Name` ends in `(BESS)`, which identifies the BESS copy in BeamNG's vehicle selector. BeamNG's [vehicle configuration documentation](https://documentation.beamng.com/modding/vehicle/tutorials/configs/) describes this display field. The v0.8.4 release downloads predate this local change and were not overwritten.

The metadata edit changes only the JSON string value for `Name`. Automation's other metadata bytes, including duplicate paint keys, remain intact. The source ZIPs are untouched. Original and BESS mods retain the same internal vehicle and audio paths, so they cannot be enabled together.

| Check | Result |
| --- | --- |
| Source archives | 12 |
| Replaced engine loops | 688; mono 48 kHz / PCM24 |
| Vehicle names with `(BESS)` | 12 |
| Other ZIP entries unchanged | 1,182 |
| Source ZIPs unchanged | 12 of 12 by SHA-256 |
| Exported ZIPs reimported | 12 of 12 |
| Full Rust test suite | 48 passed |

The active BeamNG user folder was read from `BeamNG.drive.ini`. The game was closed during installation. The twelve original mods in its active `mods` folder matched `cars` by SHA-256 and were moved to a sibling backup folder outside `mods`. Twelve exported BESS ZIPs were copied into active `mods`; their installed SHA-256 values match the exports. The final scan found twelve BESS ZIPs, zero original vehicle ZIPs in active `mods`, and twelve originals in the backup. Ten unrelated repository mods were untouched.

Local audit and restore records are `output/beamng-bess-labeled-20260923/verification.json`, `installation.json`, and `RESTORE.txt`. This confirms package structure and installation, not the visible selector result or audio behavior inside BeamNG. Driving, camera views, transitions and subjective realism remain for the in-game test under W-002.4 / S-10.
