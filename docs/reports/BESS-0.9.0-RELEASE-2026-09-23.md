# BESS 0.9.0 release and BeamNG fleet — 2026-09-23

## BeamNG installation

The active `D:\BeamMP\current\mods` folder contains twelve original Automation vehicle ZIPs and twelve matching R6 BESS configuration add-ons. Eleven older dual-layer add-ons were moved to `D:\BeamMP\current\BESS-dual-layer-variants-backup-before-r6-20260923`; the existing R6 Cerberus add-on stayed in place. All twenty-four active ZIPs passed CRC checks. The twelve original and twelve R6 SHA-256 hashes match the local R6 verification records; the old add-on hashes match the earlier installation manifest. The detailed local record is `output/beamng-r6-fleet-install-20260923/installation.json`. These vehicles remain local and are not part of a public download.

BeamNG shut down normally before installation. The new files are ready for its next launch, but this installation has not been observed in a running game. It does not establish their audible balance or naturalness.

## Release assets

The first 0.9.0 portable archive included the twelve development vehicles and their R6 add-ons. It was withdrawn after the user clarified that test vehicles must not be distributed. `tools/package_090.py` now builds an application-only archive with `BESS.exe`, a generic English `START.html`, README, release notes, and file hashes. `tools/verify_delivery_090.py` checks the complete allow-list, links, hashes, outer ZIP integrity, and standalone executable equivalence, and rejects vehicle archives, WAVs, projects, or other bundled test media. The older 0.8.4 portable archive was also withdrawn because it contained vehicle exports; its standalone executable remains available.

The asset SHA-256 values are provided in the adjacent `.sha256` sidecars so they can be verified independently of this document.

The 55 Rust tests, `cargo fmt --check`, and strict Clippy pass on the 0.9.0 source. The earlier 66-point Cerberus export-level comparison matched all measured WAV values to 0.00000001 dB or better. Those checks do not measure sound quality in BeamNG.

## Remaining listening checks

The user has not yet heard R6 in BeamNG. Several vehicles have weak inferred engine detail that BESS deliberately does not amplify without limit. A modeled Cerberus adjacent-RPM blend around 5,200 RPM shows a possible 2.35 dB exhaust level dip; BeamNG's actual crossfade is unmeasured. Compare original and BESS trims from cockpit, hood, and tailpipe cameras, then perform a slow RPM sweep around 5,200 RPM.
