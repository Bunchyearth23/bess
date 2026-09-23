# BESS 0.9.0 release and BeamNG fleet — 2026-09-23

## BeamNG installation

The active `D:\BeamMP\current\mods` folder contains twelve original Automation vehicle ZIPs and twelve matching R6 BESS configuration add-ons. Eleven older dual-layer add-ons were moved to `D:\BeamMP\current\BESS-dual-layer-variants-backup-before-r6-20260923`; the existing R6 Cerberus add-on stayed in place. All twenty-four active ZIPs passed CRC checks. The twelve original and twelve R6 SHA-256 hashes match the release manifest; the old add-on hashes match the earlier installation manifest. The detailed local record is `output/beamng-r6-fleet-install-20260923/installation.json`.

BeamNG shut down normally before installation. The new files are ready for its next launch, but this installation has not been observed in a running game. It does not establish their audible balance or naturalness.

## Release assets

`tools/package_090.py` copies the twelve verified R6 add-on ZIPs byte for byte and packages them beside the twelve original vehicle ZIPs. It updates only the adjacent portable manifests and project paths to version 0.9.0, preserving the original export version as provenance. The package has 1,376 nonempty mono 48 kHz / 24-bit PCM sound loops. `tools/verify_delivery_090.py` checked twelve source/add-on pairs, ZIP CRCs, disjoint paths, 128 file hashes, 41 start-page links, local README links, and standalone executable equivalence.

The asset SHA-256 values are provided in the adjacent `.sha256` sidecars so they can be verified independently of this document.

The 55 Rust tests, `cargo fmt --check`, and strict Clippy pass on the 0.9.0 source. The earlier 66-point Cerberus export-level comparison matched all measured WAV values to 0.00000001 dB or better. Those checks do not measure sound quality in BeamNG.

## Remaining listening checks

The user has not yet heard R6 in BeamNG. Several vehicles have weak inferred engine detail that BESS deliberately does not amplify without limit. A modeled Cerberus adjacent-RPM blend around 5,200 RPM shows a possible 2.35 dB exhaust level dip; BeamNG's actual crossfade is unmeasured. Compare original and BESS trims from cockpit, hood, and tailpipe cameras, then perform a slow RPM sweep around 5,200 RPM.
