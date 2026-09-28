# Physical scratch sound controls — 2026-09-28

The user reported that the physical scratch engine had lost too many sound-shaping controls. Nineteen independent, persisted adjustments now accompany the physical builder. The retired Descriptor voice remains absent.

## Controls and equivalents

| UI group | Controls | Effect |
| --- | --- | --- |
| Sound shaping | Bass, presence, treble; brightness cutoff | BDSP shelves at 120/4000 Hz, presence at 1200 Hz, optional low-pass. Replaces broad body/brightness/edge shaping on the physical exhaust and intake. |
| Sound shaping | Saturation; flow texture | Soft clipping without makeup gain; deterministic multiplicative filtered noise following the physical signal. No independent continuous noise source. |
| Sound shaping | Brightness at high RPM; under load | Additional high-frequency shelf driven by normalized RPM and throttle, with 30 ms smoothing. |
| Intake and mechanical character | Intake resonance; resonator length | Quarter-wave observation filter, 343/(4L) Hz. Length is disabled when resonance is zero. This filter does not claim to be a second physical intake simulation. |
| Intake and mechanical character | Mechanical pitch; resonance | Frequency and Q of the existing angle-driven mechanical layer. |
| Combustion character | Cycle variation; combustion duration; ignition retard | Scales cycle scatter, changes the burn duration while preserving its requested heat integral, and shifts ignition relative to the controller. These controls affect cylinder behavior and torque. Negative retard advances ignition. |
| Exhaust tuning | Header length; tailpipe length; muffler volume; absorption | Retunes the existing physical acoustic network. Muffler controls are disabled with no muffler. Chamber scaling follows the legacy base-length clamp to eliminate inactive ranges while preserving the default. |

Existing exhaust/intake/mechanical levels, engine-layer gain, idle gain, fuel cut, afterfire, engine construction and listening controls remain available. Part changes preserve the nineteen adjustments and the user's three layer levels and fuel-cut setting. The reset button resets only the nineteen sound adjustments. Wheel edits use the existing discrete-step helper.

## Compatibility and evidence

- A missing or partial `scratch.sound` object receives neutral defaults. Validation rejects nonfinite or out-of-range controls; project roundtrips preserve all fields.
- All nineteen controls change the relevant physical engine stem in their active operating conditions. This is a measured connection check, not a subjective audibility or realism score.
- Twelve reference WAVs (I4, cross-plane V8 and V12; exhaust, intake, mechanics and Orbit) are SHA-256 identical to the preceding corrected-idle build at neutral settings. Evidence: `output/physical-sound-controls-20260928/default-identity.json`.
- Acoustic retuning retains delayed waves, convection and sample clocks. Primary lengths slew over 50 ms; existing downstream delay smoothing remains active. A repeated neutral retune is sample-identical to an untouched network.
- Sound-only edits adopt a prepared model's controls in place. The running crank, cylinders, gas state and controller are retained; a 30 ms crossfade blends old/new tone and mechanical filters. Four rapid edits with all controls active pass a zero-allocation **and zero-deallocation** producer test. The complete unused preparation returns to the UI disposal queue.

## Listening examples

`cargo run --release --example sound_controls` reproduces three eight-second I4 clips through the same 48 kHz live render path (96 kHz synthesis), Orbit view and listening volume 0.8. Complete v3 projects are saved, reloaded, compared and then rendered; the PCM24 mono WAVs are reopened to verify frame count and nonzero finite energy.

| Variant | Adjustments | RMS dBFS |
| --- | --- | --- |
| Neutre | Defaults | -12.73 |
| Grave | Bass +6 dB, treble -4 dB, tailpipe 2.5 m | -9.12 |
| Mordant | Presence +5 dB, flow texture 0.6, saturation 0.35, intake resonance 1.5 | -12.57 |

All three reach the existing -1 dBFS output limiter during the imposed acceleration; no cross-variant level normalization is applied. These are editable demonstrations, not level-matched preference tests. Files: [listening README](../../output/physical-sound-controls-20260928/listening/README.md).

## Scope

The acoustic-to-valve feedback removed by the idle correction stays disconnected. These controls do not resolve the remaining physical calibration, subjective listening, or scratch BeamNG-export work. Imported-bank synthesis retains its own controls and export path.

## Final validation

- `cargo test --release --all-targets`: 251 tests passed, zero failures. Log: `output/physical-sound-controls-20260928/tests.txt`.
- Two subsequently added engine-retune tests passed separately: thermodynamic/shaft-state preservation, crossfade continuity and completion, identical-setting no-op, and incompatible-engine/rate rejection. Total current coverage: **253 passing tests**. Log: `output/physical-sound-controls-20260928/retune-tests.txt`.
- `cargo clippy --release --all-targets -- -D warnings` and `cargo fmt --all -- --check` passed on the final source.
- `cargo build --release --bin bess` passed. Executable: `target/release/bess.exe`, 7,600,640 bytes, SHA-256 `DDE8B63AB8D039A3666B728A8AE9FCCF1C1A418A9EE4FA17A8E70DE98A2B6CCA`.
- The already open application keeps its session. Its locked executable was renamed locally to `target/release/bess-running-before-sound-controls.exe`, allowing the usual `target/release/bess.exe` to be rebuilt without terminating that process. Save the current project and reopen the usual executable to see the controls.
- No new hardware-endurance or subjective listening result is claimed for this controls build. The earlier idle-correction device run is evidence for that earlier executable only.
