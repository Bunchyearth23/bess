# Physical crank, controls and driveline foundation — 2026-09-28

This batch supplies the crank/control components for the physical engine. Component tests do not establish that a complete engine idles, that it sounds realistic, or that the complete solver meets the audio deadline.

## Crank

`physical::crank::Crank::new(inertia_kg_m2, stroke_m, total_displacement_m3, initial_rpm)` returns a validated crank. `step(dt, gas_torque_nm, reciprocating_torque_nm, load_nm, starter)` advances its f64 angular velocity and unwrapped angle. Positive external load resists rotation; negative load allows wheel back-driving. The caller supplies cylinder gas torque and any reciprocating inertia torque.

There is no idle-restoring torque, fitted engine torque curve, redline clamp or minimum running speed. A reporting flag marks speeds below 40 rpm as stalled. The current forward-only prototype stops at zero rather than modeling reverse kickback. For constant torque within a step, the angle and velocity updates include the exact stopping instant, allowing the per-step work ledger to match kinetic-energy change even when braking reaches rest.

Estimated Chen–Flynn friction uses FMEP = A + B p_peak + C U_p + D U_p², with A=30,000 Pa, B=0.005, C=5,000 Pa/(m/s), D=400 Pa/(m/s)². Four-stroke torque is FMEP times total displacement divided by 4π. These coefficients are starting assumptions, not a measured engine calibration. A setter supplies cycle peak pressure; until supplied, the pressure term is omitted. Friction can be explicitly disabled for conservation tests.

The illustrative starter has a linear torque-speed curve, zero drive above 400 rpm, and no braking action when the engine outruns it. Accessory switches add the design report's 16 and 22 Nm loads. Neither supplies combustion torque.

## Controller

`Controller::new(idle_rpm, redline_rpm, Fuel)` and `step(dt, rpm, throttle, overrun_strength, starter)` produce throttle, bypass, fuel multiplier, spark-angle shift and spark enablement. `step_input` accepts the equivalent `ControlInput` structure.

- Idle air PI updates every 30 ms, with conditional integration at saturation and a nominal bypass area fraction of 0.060. PI gains and bypass scale remain uncalibrated.
- Spark shift is **positive for retard**. The nominal idle reserve is 10 degrees; falling speed releases that retard toward the ordinary MBT timing.
- Injection overrun enters at or above 1.8 times idle with closed throttle, and resumes fuel at idle + 200 rpm. Throttle thresholds also have hysteresis. `overrun_strength` sets the amount of retained fuel: 1 gives a complete cut, 0 disables it.
- Carburettors never receive injection-style overrun cut. Their redline protection cuts spark, retaining fuel; injected engines cut both. Rev-limit release is 150 rpm below redline. These actions affect energy input, never force shaft speed.
- Starter operation supplies modest enrichment and bypass opening. A stationary engine has no enabled spark until rotation reaches 40 rpm.

The bypass is an additional normalized throttle area. The integration layer combines it with the main opening using `1 - (1 - throttle) * (1 - bypass)` before assigning an actual flow area. The controller cannot keep an engine running unless its gas exchange and combustion actually supply enough torque.

## Physical driveline

`drive::Simulator::step_physical(controls, measured_rpm)` is an additional 1 kHz path; the existing imported-bank `step` is unchanged. It returns telemetry, signed crank load and effective driver throttle. The gearbox, road resistance, brakes and launch clutch remain, but the physical path has no fitted engine torque curve, artificial idle torque or acoustic speed ceiling. `peak_torque_nm` is only a clutch-capacity estimate.

The clutch transfers equal/opposite angular impulse through the gear ratio. `Controls.inertia` must match the physical crank's inertia for consistent coupling. Road and brake resistance cannot start reverse wheel motion. The parent integration holds the returned load over the finer crank steps. Until replaced by actual gas-state telemetry, driveline `State.load` is the effective throttle fraction, not a measured combustion load.

## Observed checks

- Five crank tests pass: analytic torque/work, passive coast to rest, starter/freewheel, accessory/friction work and invalid-input rejection.
- Seven controller tests pass: 30 ms sampling, idle reserve, overrun hysteresis, carburettor behavior, rev-limit hysteresis, integrator recovery and start/input handling.
- Five physical-driveline tests pass: untouched measured RPM including stall/overspeed, no traction from a stopped engine, clutch action/reaction and back-driving, passive coupled energy decay and brake stopping.
- `cargo clippy --lib -- -D warnings` passes for the integrated library at this stage.

These are component and coupled mechanical checks. They do not validate calibrated idle stability, complete cylinder/crank conservation across the parent solver, acoustic realism, gameplay, or long-duration device scheduling.

## Subsequent bench integration in the same batch

The parent-supplied `physical::engine::Engine` is now the default `ScratchModel::build` voice. `Bench` bypasses the old hybrid event synthesis whenever that physical engine is present. Direct/cycle modes impose the requested test-stand RPM; simulated driving passes the real crank RPM into `step_physical`, then returns the signed clutch load and effective throttle to the engine at 1 kHz. Clutch integration uses the same rotating inertia as the crank.

The listening chain, cameras and -1 dBFS limiter are retained. Exhaust, intake and mechanical controls apply to the physical sources; listening volume ramps, idle gain and a separate engine-layer gain remain controllable. A rebuilt physical engine replaces the previous one over 30 ms, after which the outgoing engine is available through the existing retirement API. The audio producer remains responsible for destruction, not the device callback. `Bench::failed()` exposes physical solver failure for device diagnostics.

The UI now labels this path “Physical engine” and exposes idle target, redline, inertia, fuel cut and the actual source gains. Obsolete standalone JSON, pulse width, event noise and manual gas-temperature controls are absent from the scratch controls. Cam revolutions are resolved before edits and can be adjusted separately from spark wiring. Legacy serialized fields remain readable; `ScratchModel::build_reference` supplies the archived event voice only for explicit offline A/reference tests.

Observed integration checks: 18 historical event-reference regressions and 5 physical bench tests pass; the shared physical worker/WAV sample stream remains bit-identical in its deterministic test; strict Clippy for the library, application and tests passes. Physical tests cover default routing and reported RPM, ignored legacy pulse controls, real output mutes, engine retirement and physical simulated RPM. These checks are not calibrated audible acceptance or a device endurance measurement of the newly integrated solver.

## Accessory controls, crossover UI and level audit

Settings now persist `accessory_ac` and `accessory_steering`, both false for older projects. They apply 16 Nm compressor and 22 Nm steering loads through physical engine commands. Changing either switch sends a live control update, without rebuilding or resetting the engine. Two tests pass: old/default and selected values round-trip through serialization; the first physical sample after applying 16, 22 or 38 Nm has the analytically expected RPM change, while the pre-step state stays unchanged. Direct RPM tests hold shaft speed externally; use simulated driving to observe accessory-induced speed changes.

Multibank engines expose Separate, H-pipe and X-pipe exhaust connections. The acoustic coupling itself belongs to the parent integration; this UI does not synthesize an extra effect.

`examples/scratch_levels.rs --stock-only` now measures the physical engine through `RenderEngine`, including the same 2x calculation and decimation as listening. It measures the final two seconds of a three-second render, with no RMS normalization. The observed CSV is `output/physical-bench-levels-20260928.csv`.

| Stock I4 condition | Volume | Position | Peak dBFS | RMS dBFS |
|---|---:|---|---:|---:|
| Idle, throttle 0.05 | 0.8 | Orbit | -9.16 | -21.51 |
| Idle, throttle 0.05 | 0.8 | Tailpipe | -1.42 | -13.15 |
| Idle, throttle 0.05 | 1.0 | Orbit | -7.22 | -19.57 |
| Idle, throttle 0.05 | 1.0 | Tailpipe | -1.00 | -11.38 |
| 4000 rpm, throttle 0.9 | 0.8 | Orbit | -14.99 | -25.39 |
| 4000 rpm, throttle 0.9 | 0.8 | Tailpipe | -5.48 | -17.57 |
| 4000 rpm, throttle 0.9 | 1.0 | Orbit | -13.06 | -23.45 |
| 4000 rpm, throttle 0.9 | 1.0 | Tailpipe | -3.54 | -15.64 |

Volume zero produces exact zero peak and power in all four measured condition/position combinations. The limiter engages at full-volume idle at the tailpipe. This measured idle is about 4 dB louder than the chosen loaded point; no automatic level correction was introduced to conceal that result. These numbers describe the compiled solver at this audit, not subjective realism or a promise that later calibration changes preserve those exact values.

## Silent device endurance CLI

`bess --audio-check REPORT scratch 600 "V12 60°" cycle` selects an exact named physical preset and runs without opening the GUI. The optional scenario is `steady` or `cycle`; steady holds 1200 rpm at throttle 0.1. Cycle repeats five three-second phases: 1200/0.1, 3000/0.8, 6000/0.8, 6000/0 lift-off and 3000/0.8 recovery. Commands are sent at most 10 times per second, independently of the internal physical sample rate. RPM is imposed by the test stand, preventing uncontrolled driveline acceleration from invalidating the endurance workload. Output volume remains zero while the solver executes.

The report identifies the engine, scenario, requested/observed duration and physical/producer failures separately. Failure preserves the full metrics report and a nonzero exit status. It also reports underruns, missing frames, callback p99 deadline percentage, callback overruns and synthesis-block time. A pure scheduling test covers every phase and repetition boundary; the actual 600-second device run is performed separately by the parent agent.

## Momentary starter and listening fixture preparation

Physical simulated driving exposes a momentary “Hold to start” button. Each UI frame clears the command before reading the held button, so leaving simulated driving or hiding the controls releases it. `Settings::starter` is excluded from both serialization and deserialization. Only the simulated physical branch passes it to the crank/controller; direct and cycle modes leave it false. Two release tests pass: project data cannot restore a pressed starter, and a stopped physical shaft gains speed through starter torque without jumping to idle or resetting on the setting change. The application and listening example pass `cargo check --release`.

`examples/physical_listening.rs` prepares three 12-second PCM24/48 kHz mono clips through the shared RenderEngine: I4, cross-plane V8 and 60-degree V12. Each contains imposed idle, acceleration, hold, closed-throttle overrun, idle recovery and a paused fadeout. It writes a local HTML player, phase-level CSV and complete parameter JSON, without normalization, and rejects nonfinite samples, physical faults or a failed final fade.

## Final corrected-engine renders

After the manifold reverse-flow correction and the final default-profile release build, the listening fixture completed for all three engines. Open `output/physical-listening-20260928/index.html`; the directory contains three WAVs, `metrics.csv` and `parameters.json`. WAV headers were independently checked: mono PCM24, 48000 Hz, 576000 frames (12 seconds). All samples were finite, no physical solver fault occurred, and the last 10 ms measured exact zero. All local audio/download links resolve. No subjective listening assessment was performed.

| Complete clip, pause included | Peak dBFS | RMS dBFS |
|---|---:|---:|
| Inline-4 | -8.68 | -23.73 |
| V8 cross-plane | -7.40 | -21.89 |
| V12 60 degrees | -7.46 | -21.60 |

The stock level CSV was regenerated with this same final release executable. These measurements supersede the earlier provisional table; the historical numbers above are retained to identify the earlier state.

| Stock I4 condition | Volume | Position | Peak dBFS | RMS dBFS |
|---|---:|---|---:|---:|
| Idle, throttle 0.05 | 0.8 | Orbit | -9.54 | -21.85 |
| Idle, throttle 0.05 | 0.8 | Tailpipe | -1.00 | -13.47 |
| Idle, throttle 0.05 | 1.0 | Orbit | -7.60 | -19.91 |
| Idle, throttle 0.05 | 1.0 | Tailpipe | -1.00 | -11.81 |
| 4000 rpm, throttle 0.9 | 0.8 | Orbit | -15.66 | -26.06 |
| 4000 rpm, throttle 0.9 | 0.8 | Tailpipe | -5.32 | -18.15 |
| 4000 rpm, throttle 0.9 | 1.0 | Orbit | -13.72 | -24.12 |
| 4000 rpm, throttle 0.9 | 1.0 | Tailpipe | -3.38 | -16.21 |

Volume zero remains exactly silent for every tested condition/camera combination. The tailpipe limiter reaches -1 dBFS at both tested nonzero idle volumes. These are offline level measurements, separate from the parent-run real-device endurance test and the still-open subjective sound validation.

## Explicit physical afterfire control and geometry diagnostics

The physical engine now consumes the existing opt-in `scratch.experimental.afterfire` setting. Above idle + 500 rpm, a throttle opening above 20% arms a single event; closing to at most 3% triggers a 200–300 ms window according to intensity. This retains a real cylinder fuel multiplier of 0.25–1, retards ignition by an additional 20–60 crank degrees and skips 1–4 of eight 720-degree ignition cycles deterministically across cylinders. Tip-in above 8%, starter use, low speed or the rev limiter cancel the window. Steady closed throttle cannot retrigger it; reset clears the armed/window state. The numeric tune is an explicit estimate, not a manufacturer calibration.

There is no generated pop, injected acoustic noise, audio gain or direct heat pulse. Unburnt fuel and fresh air traverse the existing cylinder and collector composition path. The existing aftertreatment model decides combustion from available fuel, oxygen and gas/catalyst temperature, consumes the reactants and supplies the resulting heat to the gas/acoustic model. Ordinary residual chemical reactions remain possible with the tune disabled.

Four release integration tests pass in `tests/physical_afterfire.rs`. On the tested I4 after four seconds at 4000 rpm / 80% throttle, the enabled full-intensity lift retained 0.000162371836 kg of actual injected fuel. The accumulated `Sample::afterfire_heat_j` over the following second was 1864.214874 J versus 21.269737 J with the tune disabled. This field is the acoustic afterfire excitation energy, defined as 0.8 times reacted chemical heat; it is neither total chemical heat nor net heat transferred to gas after cooling. This comparison demonstrates increased reaction activity, not a full system energy-balance proof. Before the lift, audio, injected fuel and combustion heat were identical bit for bit. Separate tests verify 210/250/300 ms windows, no repeated closed-throttle triggering, release/reset/rev-limiter precedence and readable geometry errors. This is numerical behavior evidence, not an assessment of the resulting sound.

At intensity zero the new command calculation is bypassed. Re-rendering all three original 12-second listening clips into `output/physical-listening-afterfire-regression-20260928` after the patch produced byte-identical WAV files, including headers, with these SHA256 values:

| WAV | SHA256 |
|---|---|
| i4.wav | `DEBC9DE54E91DDD86CBE5FD989B7892E9B8BFDB7B4EEF1CC510748DD615F6128` |
| v8-cross-plane.wav | `759827AAAA1A70F4D0C9E06F2BC5C093CCA1DFB5D14876AE3C78E15A973618BC` |
| v12-60.wav | `1DDB7CB933880EC14A7571271753FC5A6A23CEC66B47C8C333449AFBEBB9E22F` |

`Engine::new` now explains the existing physical geometry envelope before constructing cylinders: bore and stroke each 5–500 mm, total displacement 1 cm³–500 L. This makes unsupported input actionable; it does not expand the solver's calibrated range. Archived event-reference tests using smaller dimensions do not demonstrate physical-engine support for those dimensions.
