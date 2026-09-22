# BESS architecture (0.6 baseline)

## Data and preparation outside the audio callback

- `bank` reads the blend and decodes only its WAVs (float32 or PCM, mono/stereo reduced to mono). Entry count and sizes are bounded. Nothing is extracted.
- Loops span an integer number of 720-degree cycles with overlap joins. Correlation of average cycle profiles chooses phase offsets; multi-cycle WAV residuals are retained.
- The immutable bank is shared through `Arc`. SHA256 covers blend, names and WAV content.
- `period` refines the nominal period locally (±3%) using normalized correlation, parabolic interpolation and agreement between window halves (0.15%). Correlation threshold is 0.65; ambiguous results retain the nominal period. No firing order is inferred.
- Nominal preparation and gain are stored separately for A; B uses accepted periods. Loop storage roughly doubles, reaching about 192 MB for both copies at the 24-million-decoded-sample ceiling, plus periodic tables, temporary buffers and other state.
- A cycle-synchronous mean at approximately WAV sample density forms the periodic component, read with sinc interpolation. Residual = prepared playback minus periodic component. Separate gains are smoothed over 40 ms. Uncertain analysis makes the periodic component zero and retains the full residual.
- `project` stores parameters, hybrid settings, `SourceRef` and driving. The UI saves version 3; versions 1/2 remain readable and default to direct driving when the field is absent.

## Driving bench

- `drive::Simulator` advances in fixed 1 ms steps: engine/wheel inertia, bounded slipping clutch, ideal transmission, generic torque curve, drag, rolling resistance, positive grade, brake and resisting wheel torque.
- Manual or automatic N/1–6 gearbox, clutch opening and torque cut on shifts, automatic hysteresis and pending out-of-range downshifts. Sound-bank bounds are virtual limits, not the engine's real limiter.
- `bench::Bench` shares clock and state between CPAL playback and rendering. An integer 1,000 Hz counter is independent of audio rate. Commands are validated, and the simulation can be paused or reset. Playback allocates nothing.
- Simulated WAV rendering starts at standstill with controls held. Direct mode holds rpm/load; comparison mode adapts cycle duration. Past control movements and BeamNG physics are not exported.

## Audio synthesis

- `hybrid` owns its state, filters, sinc tables and delays. It calls BDSP `SincTable` directly: in this BDSP revision, `FractionalReader::set_ratio` can allocate a kernel for some integer ratios and is therefore not used in the dynamic callback.
- Two rpm neighbors are read per load layer, then blended by load. Crankshaft phase stays continuous; sinc interpolation adapts bandwidth to the ratio.
- The 0.6 B path used four-cycle segments with two-cycle overlap when period confidence and loop length permitted, complementary windows and integer-cycle anchors; rpm-neighbor blend weights have zero slope at endpoints. In 0.8.4, B follows the prepared loop continuously and uses a shared nominal cycle duration across neighboring samples to prevent level pumping.
- `maps` provides four 3 × 3 grids, bilinear interpolation over bank rpm and load 0–1, multipliers 0–2, neutral values for old projects and smoothed edits. CPAL and offline rendering use the same settings.
- Reference A and variation-enabled B states both continue running to support a smooth A/B transition without restarting.
- Rpm/load response, attack/release, filtered source layers, intake/mechanical texture derived from the source residual and optional turbo.
- `acoustics` has three bidirectional BDSP tubes and two section-weighted junctions (pressure continuity and flow conservation). Filtered losses, reflective open termination and sample-smoothed delays. Dimensions and effective temperature are editable and not inferred from the vehicle. Intake has its own duct and a load-dependent throttle boundary.
- Progressive fuel cut above idle retains residual pulses. Lift-off events follow a variable-interval clock with a 55 ms refractory period; damped excitation passes through the exhaust.
- Optional slow A/B energy-ratio compensation precedes an output ceiling inactive at ordinary levels.
- `engine` remains the 0.1 generic diagnostic, outside hybrid playback.

## Execution and verification

- `audio` uses CPAL, the default device and compatible f32/i16/u16 formats, bounded `Copy` commands, block processing and atomic telemetry. Its allocation counter covers `Hybrid::next/set` and `Bench::next/set`, not possible waiting inside CPAL/crossbeam. Since 0.8.3, it prefers an available 48 kHz stream and otherwise uses the device default.
- `render` uses the same Bench/Hybrid at 48 kHz with a deterministic restart and end fade. A/B comparisons are normalized offline to equal RMS with a common peak ceiling; four comparable character renders and their settings projects are also available. Exported WAVs are mono PCM24.
- `main` imports and renders on workers, rebuilding the stream after bank changes outside the callback. Import pauses playback when a new model is activated. A project is restored only when hash and parameters are valid.
- `beamng` lexically inspects metadata; it does not resolve active parts.

## Model and export limits

Version 0.8 adds `combustion`: a `Copy` configuration of up to 12 cylinders, zero for unknown, explicit angles over 720°, smooth pressure windows and exhaust-opening delay. Its excitation is combined with the source and sent through the existing acoustics. There is no gas-flow solver, identification or valve-level intake. A remains independent and the default adds nothing.

`export` prepares stable loops per rpm and load after one second of audio-engine warm-up, with cyclic overlap and common safety gain. It copies other ZIP entries unchanged, verifies the source, rejects duplicates/suspicious paths, does not overwrite an existing folder and reimports the output archive before finalizing it. Turbo, pops and driving transients are not baked into loops; game references remain intact. The manifest distinguishes technical delivery from in-game validation. Each ZIP name includes the original archive name and a short hash to avoid installed-car collisions. Tone presets preserve explicit combustion configuration.

Version 0.7 calibration measures cycle similarity and adjacent-sample difference energy relative to signal energy, corrected for sample rate. These descriptors heuristically adjust added gains on import; they do not identify geometry. A smoothed **Added color** blend controls the acoustic contribution. Existing projects are not recalibrated automatically. Two character controls generate four internal maps; old maps remain until the controls are changed.

Filtering cannot remove every source-engine defect. Phase alignment is approximate; mechanical/intake separation is not physical; exhaust dimensions are not calibrated to this vehicle. WAVs already contain exhaust coloration, so the network is not an inversion of those acoustics. Old projects use defaults for new fields, and identical 0.2 settings need not produce identical output with the 0.3 model. See [research](ENGINE-SOUND-RESEARCH.md) for per-cylinder, bank and game-export milestones.
