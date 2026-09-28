# Exhaust controls and intake/mechanical texture — 2026-09-28

## Behavior

Eight persisted **Exhaust tone** controls join the existing nineteen adjustments, in the shared scratch/import interface: bass (±12 dB), body gain (±12 dB), body frequency (40–2000 Hz), body Q (0.5–8), rasp (±12 dB), low cut (20–300 Hz), high cut (500–20000 Hz), and saturation (0–1). Body frequency/Q become editable when body gain is active. A dedicated reset restores these eight values without changing the other tuning, parts or layer levels. Default values bypass this shaping, including when loading older projects. The 20 Hz/20 kHz cutoff endpoints mean bypass.

These controls shape the physical exhaust observation only, after the existing shared tone controls. They do not modify intake, mechanics or gas/shaft behavior. Prepared live retunes retain the running engine and use its existing 30 ms DSP crossfade. The same settings reach live listening, WAV and imported BeamNG rendering.

The user subsequently reported saw-like intake and mechanical layers. The previous intake was dominated by repeated valve-flow pulses: its cubic flow-noise term was extremely small. Mechanics used identical impulses through one resonator. The replacement observation damps sharp intake flow edges and combines them with band-limited breath driven by the real flow envelope. Mechanical events excite 1.5 ms decaying textured contacts with a small coherent onset; existing pitch/resonance controls still shape them. No independent oscillator or alternative rendering mode was added. Without flow/contact events, the new observation is silent after its tails decay. Its deterministic per-engine seed is cached at construction so restart does not serialize metadata or allocate.

This intentionally changes the default intake/mechanical timbre of existing physical projects. Exhaust defaults and original A remain unaffected by this observation correction. The observation is a perceptual model, not a calibrated measurement of an actual intake or engine block.

## Verification

- **217 release all-target tests passed**, including measured exhaust EQ/cut/drive response, stem isolation and unchanged physical state, all 27 control effects, persistence/reset, bounded extremes and neutral bypass.
- Deterministic texture, cycle-to-cycle differences, silence/tail decay at 8–384 kHz, real-time restart and retune with zero allocations, physical live/WAV identity, original A integrity and source-independent B all pass.
- Strict all-target Clippy, formatting and release build passed. Logs: `output/exhaust-controls-tests.txt`, `output/exhaust-controls-clippy.txt`, `output/exhaust-controls-build.txt`.
- Imported-V8 default-device check passed for 30 seconds at 48 kHz float stereo: 3000 callbacks, zero underruns, missing frames, budget overruns or solver/producer failures. Maximum 10 ms synthesis block took 8.528 ms. Output was muted; this proves streaming behavior, not an audition. Report: `output/engine-layers-20260928/audio-check.txt`.
- Eight isolated mono 48 kHz PCM24 clips are in `output/engine-layers-20260928/{before,after}`. Each records 3 seconds after 1 second of warmup; default I4, 850 RPM/.1 load and 3000 RPM/.7 load. Both revisions use fixed gain ×16 and listening volume .8, the same output protection, and no normalization. Reproduce the current clips with `cargo run --release --example engine_layers -- output/engine-layers-20260928/after`.

Measurements use the final two seconds, remove DC and measure spectral energy in ±1.1 Hz windows around multiples of the 720° cycle rate. This is a texture indicator, not a sound-quality score. Full metrics are in `output/engine-layers-20260928/metrics.json`.

| Isolated layer | Before RMS dBFS | After RMS dBFS | Before harmonic-window energy | After harmonic-window energy | After peak dBFS |
| --- | ---: | ---: | ---: | ---: | ---: |
| Idle intake | -43.50 | -46.12 | 99.9% | 51.1% | -31.72 |
| Idle mechanics | -60.38 | -60.68 | 99.8% | 33.7% | -38.20 |
| Loaded intake | -25.50 | -29.38 | 100.0% | 45.2% | -16.87 |
| Loaded mechanics | -49.41 | -48.94 | 100.0% | 10.4% | -31.38 |

The reduced periodic concentration is not obtained by simply raising layer volume. Mechanics retains approximately the former RMS level; intake is somewhat quieter and less dominated by coherent pulses. These files show the technical change; the user must still judge naturalness and balance in the complete sound and BeamNG.

## Delivery

Rebuilt `target/release/bess.exe`: 7,517,696 bytes, SHA256 `a8d2fa1fffcbb5db372433de151da3710709a0798af93d51dc4c8a09d86826d9`.

The running application was preserved under a dated executable name. Reopen the usual executable to load this revision. No commit, push, mod installation or subjective listening acceptance is claimed.
