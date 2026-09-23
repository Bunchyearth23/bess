# BESS 0.10.1

This correction keeps the experimental independent generator in the listening interface and restores the standard source-guided sound for BeamNG add-ons. New imports start in the standard mode. Existing projects keep their saved listening mode, but **Create BeamNG configuration** always exports the standard mode. The BeamNG two-emitter preview and volume estimate use the same standard path. The add-on converter rejects a previously rendered experimental bank.

The six character presets from 0.10.0 remain available: Balanced, Muted, Open, Warm, Mechanical and Grit. Generated body, edge, flow and mechanical controls apply only to experimental listening. BeamNG configurations still add separate engine-side and exhaust emitters beside the original Automation trim, with mono 48 kHz / 24-bit PCM WAVs.

## Windows package contents

- `BESS-0.10.1-Windows-Portable.zip`: application, English instructions, release notes and verification hashes.
- `BESS-0.10.1-Windows.exe`: standalone application.

No test vehicles, source sound banks, sample projects or prepared BeamNG add-ons are included in the application packages. Import your own Automation ZIP, export its add-on, and keep the original vehicle ZIP beside it. Remove an older BESS add-on for the same vehicle before installing the new one.

File and signal checks cannot establish in-game naturalness or spatial balance. Compare the original and `(BESS)` trims in BeamNG. The public source depends on the pinned private BDSP repository; the Windows binaries run without repository access.
