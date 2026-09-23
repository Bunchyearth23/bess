# Standalone procedural engine instrument

This is the recording-free path in `src/standalone.rs`. It supplements BESS's Automation-guided sound path; the existing BeamNG export and project format are unchanged. The model is a playable four-stroke approximation, not a thermodynamic or certified acoustic predictor.

## Boundaries and clock

- `Config` (JSON version 1) describes 1–12 named cylinders, firing phases, bank routing and two opening offsets. `Calibration` contains sound controls rather than measured mechanical dimensions. `Commands` contains rpm, normalized load, user volume and explicit `firing`, `motoring` or `fuel_cut` state.
- The engine owns a continuous `f64` cycle position. One cycle is 720 crank degrees; `cycle_hz = rpm / 120` and `rotation_hz = rpm / 60`. A cylinder fires once per cycle. The average combustion event rate is `N * rpm / 120`, which is not necessarily the strongest spectral peak.
- Each audio sample checks the half-open phase interval between its old and new cycle positions. A small numerical tolerance assigns a mathematically exact end-boundary event to the next sample. Changing block size does not reset phase. At commanded zero rpm the phase freezes immediately, while delay and resonator energy continue to decay.
- Rpm, load, volume and the three source levels smooth over approximately 25 ms. A command change never rebuilds the topology. The first version treats rpm as an external command; throttle, torque, inertia and vehicle load do not determine it.

## Excitation and paths

- Per-cylinder firing, exhaust-opening and intake-opening pulses use separate angles. The two opening offsets are deliberately coarse timing assumptions, not valve events inferred from a vehicle. `motoring` has a weak compression/mechanical pulse without combustion; `fuel_cut` retains intake and exhaust pumping events without that pulse.
- Smooth raised-cosine pulses are at least eight samples wide. A fixed-seed generator varies strength once per combustion event. Pulse-linked noise is low-passed before mixing; there is no independently randomized parameter modulation every sample. Names have no DSP effect. Phases, strengths and acoustic routes do.
- Each bank has distinct intake and exhaust one-way delay lines. `delay_samples = sample_rate * length_m / sound_speed_m_s`; linear interpolation handles fractional samples. Signed reflections are multiplied by loss, with the product bounded below 0.68 in magnitude. This is a reduced reflection network, not a geometric exhaust solver. Effective sound speed is user supplied over 250–700 m/s; 420 m/s in presets is a sound design assumption, not a universal exhaust-gas property.
- The block is a damped second-order resonator at an editable calibration frequency. A first-order 25 Hz DC blocker follows the summed paths. Output is mono. The live tool duplicates that same mono sample across the device channels; it does not spatialize it.
- The pulse width provides a proportional first anti-alias measure; high-frequency residual noise is filtered, and the ordinary path has no nonlinear waveshaper. A hard safety ceiling at ±0.98 remains for pathological settings and can itself alias if reached. The supplied 48 kHz validation clips never reached it. No oversampling or formal alias rejection measurement is claimed.

## Real-time and offline use

`Synth::new` validates configuration and commands and allocates all duct buffers. `next_sample` and `render_block` use only owned preallocated state. The WAV CLI and CPAL live CLI call this same DSP. The live tool receives bounded messages with nonblocking polling at callback boundaries; disk reads, JSON decoding and topology construction occur before stream startup. Its source-level edits are smoothed. The WAV CLI streams 24-bit mono PCM without independent normalization, preserving level differences between scenarios.

Presets `single`, `four-even` and `four-split` are illustrative. The last two have four cylinders but differ in phasing and bank routing. They are not validated copies of commercial engines.
The `--compare` command renders all three under identical controls without per-file normalization, so level and timbre changes remain visible.
