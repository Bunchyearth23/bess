# BESS 0.10.0

BESS now starts new Automation imports with an independent generated engine sound. The exhaust WAV bank supplies measured level, cycle and broad spectral descriptors; its PCM waveform is not replayed in the generated B sound. Engine-side, intake and flow detail are modeled from exhaust-only input, so their character remains an estimate.

Six character presets are available: **Balanced**, **Muted**, **Open**, **Warm**, **Mechanical** and **Grit**. Generated exhaust body, edge, flow texture and mechanical detail also have individual controls. The presets preserve engine timing and driving response. Existing projects retain their saved sound mode and gain neutral defaults for the new controls.

The BeamNG export keeps the original Automation vehicle selectable and adds a **(BESS)** configuration with separate exhaust and engine-side emitters. Export WAVs remain mono **48 kHz / 24-bit PCM**. The level panel reports file measurements, not guaranteed in-game loudness.

## Windows package contents

- `BESS-0.10.0-Windows-Portable.zip`: application, English instructions, release notes and verification hashes.
- `BESS-0.10.0-Windows.exe`: standalone application.

The public downloads contain no test vehicles, source sound banks, sample projects or prepared BeamNG add-ons. Import your own Automation ZIP and export its add-on. Keep the original vehicle ZIP beside the BESS add-on, and remove an older BESS add-on for the same vehicle before using the new one.

The generated sound is an experimental model. In-game listening remains the test of naturalness, transitions and balance. The public source depends on the pinned private BDSP repository; the Windows binaries run without access to that repository.
