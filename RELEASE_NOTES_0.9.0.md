# BESS 0.9.0

BESS 0.9.0 adds selectable BeamNG configurations with separate engine-side and exhaust sound layers. It keeps each original Automation vehicle and trim available. The engine-side layer is inferred from Automation's exhaust recordings: a source-derived residual and restrained, slightly varied mechanical impacts reduce the regular drone of the first two-layer prototype. The sounds remain a synthesis, not an isolated engine-bay recording.

The **BeamNG volume estimate** panel measures the exact exported 48 kHz / 24-bit WAV levels at every RPM and load point. It compares BESS exhaust with the original Automation sound and the added engine WAV with the BESS exhaust. Move **Inspect at RPM** to view adjacent export points. These file measurements cannot predict the final loudness from BeamNG's vehicle gains, cabin filtering, camera position, and spatial mix.

## Windows downloads

- `BESS-0.9.0-Windows-Portable.zip`: application, English setup instructions, release notes, and verification hashes. No test vehicles, sound banks, projects, or prepared vehicle add-ons are included.
- `BESS-0.9.0-Windows.exe`: standalone application. Import your own Automation vehicle ZIP and use **Create BeamNG configuration** to make its add-on.

Install your own original Automation vehicle ZIP and the BESS add-on you export from it together in BeamNG's `mods` folder. If the original is already installed, keep it and install only the BESS add-on. Remove an older BESS add-on for the same vehicle before enabling the new one; do not keep two `(BESS)` variants for one trim. Select the `(BESS)` configuration in BeamNG's vehicle selector. The add-on does not change vehicle physics.

## Verification and limits

Development checks on twelve local vehicles covered 1,376 mono 48 kHz / 24-bit PCM WAVs, ZIP integrity, unique paths, source hashes, and separation from the originals. Those vehicle files are not distributed. The 55 Rust tests and strict Clippy checks pass. A 66-point export-level analysis matched actual exported WAV measurements.

The new mechanical layer has not yet received a user listening verdict in BeamNG. Some vehicles have a quiet inferred engine layer because BESS limits amplification of weak source detail. A simulated Cerberus adjacent-RPM crossfade shows a possible level dip around 5,200 RPM; BeamNG's actual crossfade and the audible result still need an in-game check. The live A/B audition is a mixed preview and does not reproduce BeamNG's separate engine-bay and tailpipe emitters.

The BDSP source dependency is private. Building this public source checkout requires access to BDSP; the Windows downloads run without repository access.
