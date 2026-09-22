# Selectable BeamNG configurations — 2026-09-23

The local BeamNG installation now has the twelve original Automation vehicle ZIPs and twelve BESS configuration add-ons active together. Each original model keeps its original configuration, and its add-on provides a second configuration named `<trim> (BESS)`; Cerberus A, for example, gains `A (BESS)` under the same vehicle model. This follows BeamNG's [configuration format](https://documentation.beamng.com/modding/vehicle/tutorials/configs/) and [part/slot system](https://documentation.beamng.com/modding/vehicle/intro_jbeam/partslotsystem/).

Each add-on contains a new `.pc` and matching `info_<config>.json`, a clone of only the primary engine JBeam part with a unique part key, a unique sound blend and WAV paths, and a copy of the configuration thumbnail. The cloned part retains the original physics and child slots while selecting the BESS sound bank. The base `info.json`, original configuration, engine part and sound files are not overwritten. The previous full-replacement ZIPs are now in an inactive backup outside `mods`.

| Check | Result |
| --- | --- |
| Original vehicles and BESS add-ons active | 12 + 12 |
| BESS engine loops | 688; mono 48 kHz / PCM24 |
| Add-on ZIP entries | 748 unique paths |
| Internal path overlaps | 0 between add-ons, originals or ten unrelated repository mods |
| Installed ZIP hashes | Every active original matches its source; every active add-on matches its export |
| Earlier full replacements | 12 preserved outside active `mods` |
| Current CLI export smoke test | Cerberus add-on produced with no staging files left |
| Rust test suite | 49 passed |
| BeamNG startup and mod manager | Twelve originals and twelve add-ons mounted and marked active; old replacements absent |

The batch verifier checks that every BESS configuration selects its new engine part, the engine references its new blend, the blend references every included WAV, each WAV has the specified format, the thumbnail matches the source, and no add-on entry replaces an original path. After installation, BeamNG's startup log mounted all 24 ZIPs and its mod database marked the twelve originals and twelve add-ons active. The local `output/beamng-bess-variants-20260923` folder contains per-vehicle manifests, `verification.json`, `installation.json`, and `RESTORE.txt`.

The existing v0.8.4 release downloads predate this configuration export; current source, local add-ons and `dist/BESS-variant-preview-20260923/BESS.exe` implement it. Package, mount and mod-manager checks do not establish that BeamNG displays or plays every configuration correctly. Driving, camera views, sound transitions and subjective realism remain to be tested in game under W-002.4 / S-10.
