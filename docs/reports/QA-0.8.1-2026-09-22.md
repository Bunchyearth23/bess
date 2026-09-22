# BESS 0.8.1 — Final workflow audit

Delivery: `dist/BESS-0.8.1-complete/START.html`.

## Audit fixes

- **Balanced**, **Muted** and **Open** previously erased cylinders and angles entered under **Engine and combustion**. They now preserve that configuration, A/B mode, level compensation and response. A regression test uses custom angles.
- All twelve 0.8 B tracks were byte-identical to 0.7 C tracks. Combustion is optional and inactive on import, so this was expected. The page no longer offers a redundant C comparison and explains where to hear three genuine combustion demos. Duplicate C WAVs were removed. A/B files retain the 0.8.0 render label; 0.8.1 fixes the workflow and packaging without changing default sound.
- All twelve BeamNG packages had the same external filename. Export now derives a unique name from the vehicle and a short hash and records it in each manifest and guide. Existing copies were renamed without changing ZIP contents. Multiple BESS cars no longer overwrite one another merely because of filename collision.

## Verification and limit

- 41 release tests passed; formatting and strict linting reported no warnings.
- Twelve copies have `art/` and `vehicles/` roots; all 688 WAVs referenced by blends are mono PCM24 at 48 kHz, with no missing or duplicate internal paths. The other 1,194 entries match original Automation ZIPs.
- The 0.8.1 executable's `--beamng` command produced a fresh Genesis Phantom copy. Its 189 decompressed entries matched the delivered copy, and its distinct ZIP name and guide were checked.
- The delivery contains 81 verified local links, 39 projects with relative source paths, twelve unique ZIP names and 178 SHA256-inventoried files.
- BeamNG.drive 0.39.4.0 was locally installed. The configured mods folder held all twelve originals, and no BESS copy was installed. The existing game log predated this delivery, so neither in-game recognition nor sound was proven. The [test guide](../FINAL-TESTS.md) uses the active mods folder and one enabled version per vehicle.

Structure and installation follow official [BeamNG ZIP mod packaging](https://documentation.beamng.com/modding/mod-support/mod_packing/) and [mod installation](https://documentation.beamng.com/tutorials/mods/installing-mods/) guidance. Subjective listening and in-game driving remain S-10.
