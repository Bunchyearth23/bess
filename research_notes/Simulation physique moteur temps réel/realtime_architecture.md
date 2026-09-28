# Real-time physically simulated engine audio: architecture, numerics, and a survey of existing projects

Scope: architecture for a crank-angle-resolved gas-dynamics engine that feeds a real-time audio callback (Rust, cpal, 48 kHz), plus lessons from open-source physics-based engine sound simulators. Researched 2026-09-28. Where a number comes from source code, the file is named. Engine-sim facts were checked against a shallow clone of `ange-yaghi/engine-sim` (master) made on that date.

---

## 1. Simulation rate, substeps, integrators, and flow limiting

### Takeaway
The main open-source reference, engine-sim, runs its rigid-body step at a **fixed 10 kHz by default**. It runs **8 explicit first-order gas substeps per step** (80 kHz gas rate). It keeps the explicit gas scheme stable by **hard-clamping the moles transferred per substep** (at most 0.9 × the source contents, and never past pressure equilibrium). Per-engine rates range from **5 kHz (V12) to 40 kHz (single cylinder)**, which shows the rate is set by CPU budget, not accuracy. The more rigorous 1D-duct projects (EngineLab, open-racing) use finite-volume cells for the low band and add **digital waveguides above a crossover** because grid numerical diffusion kills high frequencies.

### Cited Findings
- engine-sim's default simulation frequency is `m_simulationFrequency = 10000` (Hz). Steps per frame are `m_steps = round((dt * m_simulationSpeed) / timestep)`. The rigid-body system is processed once per step with `m_system->process(timestep, 1)` — [engine-sim src/simulator.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/simulator.cpp)
- The rigid-body solver is `OptimizedNsvRigidBodySystem` with `GaussSeidelSleSolver`, or `GenericRigidBodySystem` with `GaussianEliminationSleSolver` + `NsvOdeSolver`. It is a constraint-based rigid-body system (crank, rods, pistons), not an analytic slider-crank — [engine-sim src/simulator.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/simulator.cpp)
- Gas substeps: `m_fluidSimulationSteps = 8`, `fluidTimestep = timestep / m_fluidSimulationSteps`. Each substep processes, in order: exhaust systems, intakes, and combustion chambers' `flow()`. At 10 kHz this makes an 80 kHz gas update — [engine-sim src/piston_engine_simulator.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/piston_engine_simulator.cpp)
- Per-engine `simulation_frequency` values in the shipped `.mr` engine scripts (checked by grepping the cloned repo, `assets/engines/`):
  - Honda TRX520 single: 40000
  - Harley Shovelhead: 35000
  - Kohler CH750: 30000
  - Hayabusa, Honda VTEC, and all Subaru EJ25: 20000
  - Audi I5: 17000
  - Radial-5: 12000
  - GM LS, 2JZ, and Ferrari F136 V8: 10000
  - Radial-9: 7500
  - Merlin V12: 7000
  - LFA V10: 6500
  - Ferrari 412 T2 V12: 5000

  Source: [engine-sim repo](https://github.com/ange-yaghi/engine-sim). The default in `es/objects/objects.mr` is 10000.
- The user can change the simulation frequency at runtime (N + scroll), clamped to **400–400 000 Hz** — [engine-sim src/engine_sim_application.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/engine_sim_application.cpp). The FAQ warns that lowering it "too much might cause the simulation to become unstable" — [engine-sim wiki FAQ](https://github.com/ange-yaghi/engine-sim/wiki/Frequently-Asked-Questions)
- engine-sim's gas flow scheme:
  - It is explicit first order.
  - It uses isentropic nozzle flow with choked-flow detection (`if (p_ratio <= chokedFlowLimit) flowRate = chokedFlowRateCached ...`).
  - It limits the flow to the pressure-equilibrium maximum (`if (abs(flow) > abs(maxFlow)) flow = maxFlow`).
  - It clamps positivity: `flow = clamp(flow, 0.0, 0.9 * source->n())`.
  - It carries directional dynamic pressure and momentum, and transfers them in proportion to the fraction of moles moved.
  - It clips kinetic energy at zero (`if (E_k < 0) E_k = 0`).

  Source: [engine-sim src/gas_system.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/gas_system.cpp)
- engine-sim's exhaust is **not** a 1D duct. `ExhaustSystem::process` is a single lumped `GasSystem` (volume = collector area × length). It flows to an "atmosphere" with a 10 m² outlet area and applies a `velocityDecay`. Wave propagation is approximated in the audio path with per-cylinder delay lines of length (primary + exhaust length) / 343 m/s — [engine-sim src/exhaust_system.cpp](https://github.com/ange-yaghi/engine-sim), [src/piston_engine_simulator.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/piston_engine_simulator.cpp)
- open-racing (Rust) engine-sim PR #13:
  - It replaced first-order upwinding in its duct finite-volume solver with a **second-order MUSCL–Hancock** scheme. The first-order scheme's numerical diffusion "cut the tailpipe sound by about 15 dB between 2000 and 6000 rpm", and 400 Hz "lost ~20 dB over the exhaust's 5 m".
  - The second-order scheme costs "a quarter of the speed". The authors partly paid for it by consolidating gas-property lookups into one grid operation.
  - A digital-waveguide layer covers frequencies above a crossover of **350 Hz in Draft quality, doubling with each finer quality**. Draft sound bandwidth reached 10.8 kHz.
  - The grid alone "loses about 9.5 dB at 600 Hz across 3 m of pipe". The waveguide agrees with the cells "within about 2 dB at the exhaust's low orders".
  - Weak-shock steepening is limited to |u'| ≤ c/5.

  Source: [r4ai/open-racing PR #13](https://github.com/r4ai/open-racing/pull/13)
- EngineLab (C++20/JUCE):
  - It uses "0-D cylinder chambers and low-band quasi-1-D networks".
  - Its "conservative quasi-1-D intake and exhaust: every duct is meshed and solved, with signed SI mass flow at the valves, characteristic waveguides, a thermal wall model", plus a passive radiation load at the outlet.
  - The README states no timestep, cell count, or CFL number.

  Source: [zolaski333/EngineLab](https://github.com/zolaski333/EngineLab)
- ignis (C++20) runs its physics "at the audio sample rate" with a 0D lumped per-cylinder gas model: polytropic compression/expansion, Wiebe burn, and no port-flow modelling, since intake/exhaust strokes "relax toward manifold pressure". It has no 1D exhaust — [xevrion/ignis](https://github.com/xevrion/ignis)
- Background on explicit Euler: for a decay mode of rate k, it is stable only for h < 2/k, so the fastest time constant bounds the step. Implicit Euler has no such limit — [Wikipedia: Stiff equation](https://en.wikipedia.org/wiki/Stiff_equation)

### Inferences
- **Crank angle per step** (computed; one crank rev = 360°, 9000 rpm = 54 000°/s):
  - 10 kHz: 5.4°/step. engine-sim at 10 kHz with 8 substeps: 0.675° per gas substep.
  - 48 kHz: 1.125°/step. 96 kHz: 0.56°/step.
  - A fixed **0.25° crank step** would need 216 kHz at 9000 rpm, but only 19.2 kHz at 800 rpm idle.
  - The engine-sim LFA V10 at 6500 Hz × 8 substeps runs about 1.04° per gas substep at 9000 rpm, and 8.3° per rigid-body step.
- **Fixed-dt vs crank-angle stepping.** Both couple back to rpm; the difference is where the variability lands.
  - Fixed dt keeps the audio pipeline trivial: an integer or fixed-ratio relation to 48 kHz. It also makes CPU cost constant, independent of rpm. But angular resolution degrades at high rpm.
  - Crank-angle stepping keeps angular resolution constant. But CPU cost scales linearly with rpm, and the output is non-uniformly sampled in time, so it needs a variable-ratio (asynchronous) resampler to reach 48 kHz.
  - For a real-time callback with a hard deadline, fixed dt sized for the redline worst case is the simpler, safer default.
  - A hybrid also works: fixed audio-rate outer step, with gas substeps N(rpm) = ceil(Δθ_per_step / Δθ_target), capped. This keeps the output uniformly sampled.
- **Stiffness source.** The stiff mode is small volumes (runners, ports, primaries) connected through large effective orifice areas (open valves). The relaxation rate ~ (flow coefficient × area × c) / V can exceed 1/dt.
  - engine-sim's answer is not an implicit solver. It clamps the transferred amount to the equilibrium limit and to 0.9 of the source contents. This makes explicit Euler unconditionally positive and non-overshooting, at the cost of accuracy when the clamp is active.
  - The same trick ports directly to Rust. The alternatives are an implicit or linearised-implicit update for the orifice pair (a 2×2 solve), or more substeps.
- **CFL for 1D ducts** (worked example, not sourced): Δt ≤ Δx / (|u| + c). With hot exhaust c ≈ 600–700 m/s plus u ≈ 100 m/s, a 48 kHz step (20.8 µs) needs Δx ≳ 15–17 mm, and 96 kHz needs Δx ≳ 8 mm.
  - A 1 m primary at 2 cm cells is 50 cells.
  - Numerical diffusion at practical cell sizes removes the upper band, which is exactly the problem open-racing hit.
  - This supports the **FV-low-band + waveguide-high-band** split used by both open-racing and EngineLab, over running FV at extreme rates.
- **Explicit RK2/RK4 on the 0D chambers** buys accuracy, not positivity. The positivity clamp is still needed on valve/orifice fluxes. Semi-implicit (symplectic) Euler is adequate for the crank angular state.

### Gaps
- No source found giving EngineLab's actual timestep, cell count, or cells per duct. The README omits them, and the source was not inspected.
- No published µs-per-step benchmark for engine-sim or EngineLab was found. EngineLab only reports a "realtime factor" at runtime, and open-racing reports 0.77× real time for Draft on "a loaded machine", with no CPU model.
- No primary source on the stable timestep for valve-flow stiffness in 0D engine models specifically; only generic ODE stability theory was found.
- Bilbao's *Numerical Sound Synthesis* (energy-based stability for nonlinear FD schemes) is the canonical reference but was not fetched in this pass.

---

## 2. Crankshaft/rotational coupling, load, starter, stall

### Takeaway
engine-sim treats the crank, rods, and pistons as **constrained rigid bodies** in a Gauss-Seidel/NSV constraint solver. The starter motor, dynamometer, and vehicle/transmission are attached to the output shaft as constraints. Ignition events are driven from crank angle every step, so rpm feeds back into timing naturally. EngineLab and ignis also derive torque from cylinder pressure every cycle.

### Cited Findings
- In engine-sim, the starter motor and dynamometer connect to the output crankshaft through constraint-based coupling, which provides vehicle acceleration and load. The ignition module updates each step (`im->update(timestep)`) and fires per-cylinder ignition events that trigger chamber combustion — [engine-sim src/piston_engine_simulator.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/piston_engine_simulator.cpp)
- EngineLab has "slider-crank kinematics, injection, flame propagation, torque derived from cylinder pressure, friction and pumping losses, turbocharging" through to the vehicle driveline, with ECU-driven emergent cycle-to-cycle variability — [zolaski333/EngineLab](https://github.com/zolaski333/EngineLab)
- ignis solves "crank, rod, and piston dynamics as constrained rigid bodies" with a sequential-impulse solver with split-impulse position correction written for the project — [xevrion/ignis](https://github.com/xevrion/ignis)

### Inferences
- A general constraint solver is heavier than needed for audio. A **1-DOF crank model** is cheaper and deterministic:
  - State: θ, ω.
  - Torque: T = Σ (p_cyl − p_crankcase)·A_piston·dx/dθ − T_friction(ω) − T_load.
  - Inertia: an equivalent J, with J(θ) variation optional.
  - Integration: semi-implicit Euler at the gas step rate.
  - Piston position/velocity comes from the analytic slider-crank formula.

  The constraint approach only pays off if rod/piston inertial torque ripple or flexible coupling must emerge on its own.
- **Load/vehicle:**
  - Clutch as a friction-limited torque.
  - Gearbox ratio reflected inertia, J_eff = J_engine + J_vehicle / (i_gear · i_final)² when locked.
  - Starter as a torque source with a speed-dependent curve that drops out above a threshold rpm.
  - Stall when ω ≤ 0: clamp ω at 0 and stop ignition/injection.
- **Timing feedback:** ignition, valve lift, and injection should be functions of θ (mod 720°) evaluated each substep. Ignition should interpolate the crossing inside a step (fractional-step event) rather than snapping to the step. Snapping produces step-quantised combustion onset, which is audible as jitter at low sim rates: at 10 kHz and 9000 rpm, a step is 5.4°.

### Gaps
- No source found quantifying the audible effect of event quantisation versus sub-step event interpolation in engine-sim or its forks.

---

## 3. Real-time audio architecture: threading, buffering, latency, CPU, denormals, determinism, live parameter changes

### Takeaway
None of the surveyed projects computes the heavy gas model inside the audio callback.
- engine-sim runs physics on the frame/main loop and synthesis on a dedicated audio-render thread, joined by mutex/condvar-guarded buffers. It targets **0.1 s** of input latency, closing the loop by scaling the number of physics steps per frame by ±10%.
- EngineLab runs a separate physics thread. Its callback does "no allocation, no locking and no file access", and data crosses SPSC queues.
- ignis runs a physics thread feeding a lock-free ring drained by the SDL callback.

In Rust this maps directly onto `rtrb` (wait-free SPSC) between a physics thread and the cpal callback.

### Cited Findings
- engine-sim threading and latency (all from the cloned repo):
  - Synthesis runs on a dedicated `std::thread` (`audioRenderingThread`). Input/output handoff uses `std::mutex` + `std::condition_variable` ([synthesizer.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/synthesizer.cpp)).
  - The render thread tops the output buffer up to **2000 samples** (`m_audioBuffer.size() < 2000`) (synthesizer.cpp).
  - The target synthesizer latency is `m_targetSynthesizerLatency = 0.1` s ([simulator.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/simulator.cpp)).
  - In `startFrame`: if latency is below target, `m_steps = (m_steps + 1) * 1.1`; if above, `m_steps = (m_steps - 1) * 0.9` (simulator.cpp).
  - The platform audio buffer is 44100 samples, with the write pointer at a 10% (100 ms) lead and a safety threshold at 50% lead (engine_sim_application.cpp).
- engine-sim's output format is **44.1 kHz, 16-bit, mono**: `params.m_bitsPerSample = 16; m_channelCount = 1; m_sampleRate = 44100` — [engine_sim_application.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/engine_sim_application.cpp)
- The FAQ acknowledges "physics and sound calculations can be very resource intensive", and some engines make the app "lag or jerk, or the sound keeps repeating". That is underrun/repeat behaviour when physics falls behind — [engine-sim wiki FAQ](https://github.com/ange-yaghi/engine-sim/wiki/Frequently-Asked-Questions)
- EngineLab:
  - "The real-time audio callback performs no allocation, no locking and no file access; physics telemetry crosses SPSC queues."
  - A `RealtimeBudgetHarness` measures "simulated seconds produced per wall-clock second".
  - "Large engines (V8, V12) remain expensive for the physics thread. The application displays its realtime factor so that cost is visible."
  - Maximum 8 audio exhaust paths.

  Source: [zolaski333/EngineLab](https://github.com/zolaski333/EngineLab)
- ignis: "a lock-free ring" decouples the physics thread from SDL's audio callback. The callback applies DC removal, a pressure-derivative edge, and delay-line pipe resonances — [xevrion/ignis](https://github.com/xevrion/ignis)
- open-racing runs the live engine on its own thread and lets it settle at idle silently for 1.5 s before it becomes audible. Output is 48 kHz — [r4ai/open-racing PR #13](https://github.com/r4ai/open-racing/pull/13)
- `rtrb` is a wait-free SPSC ring buffer for real-time use (audio). All read/write functions return immediately. Its code derives from crossbeam PR #338 — [mgeier/rtrb](https://github.com/mgeier/rtrb), [rust.audio announcement](https://rust-audio.discourse.group/t/announcement-real-time-ring-buffer-rtrb/346). The documented cpal pattern: the callback only moves f32 samples across an rtrb ring and never blocks, allocates, or locks — [trem_cpal docs](https://docs.rs/trem-cpal/latest/trem_cpal/)
- `no_denormals` provides an RAII guard that sets the FTZ and DAZ bits in MXCSR on x86/x86_64 and restores them on drop — [docs.rs/no_denormals](https://docs.rs/no_denormals)

### Inferences
- **Recommended Rust topology:**
  1. **Physics thread.** Runs the gas + crank model at a fixed internal rate and pushes mono or multi-channel f32 into an `rtrb::RingBuffer`. It sits in a pacing loop keeping ring fill near a target, e.g. 2–4 callback blocks (~10–40 ms). This is engine-sim's closed-loop latency control, done on fill level instead of per UI frame.
  2. **Parameter path.** A second `rtrb` carries parameter/command messages (throttle, rpm target, geometry swap) from UI to physics.
  3. **cpal callback.** Pops samples, applies cheap post-processing (DC blocker, final limiter), and on underrun outputs a short fade to the last value or silence instead of repeating the buffer. Repeating the buffer is engine-sim's audible failure mode.
- **Physics inside the callback** is viable only if worst-case cost per block is safely below the block period. At 48 kHz and 256 frames, a block is 5.33 ms. Running the gas model in the callback also couples physics jitter to xruns.
  - The separate-thread design adds a latency equal to the ring fill, e.g. 20 ms. That is acceptable for an engine sound: engine-sim tolerates 100 ms.
  - It turns occasional physics spikes into buffer drain instead of dropouts.
- **CPU budget arithmetic** (not sourced):
  - At a 96 kHz internal gas rate, each step must finish in < 10.4 µs of wall time for 1× real time. With a 50% safety margin that is ~5 µs per step for the whole engine.
  - For 12 cylinders × (chamber + intake runner + exhaust primary) ≈ 36 0D volumes plus maybe 200–400 FV cells, that is roughly 10–25 ns per volume/cell per step. This is feasible only with tight, allocation-free, SoA/SIMD code.
  - This is consistent with engine-sim dropping to 5–7 kHz for V10/V12 engines, and EngineLab flagging V8/V12 as "expensive".
- **SIMD:**
  - Per-cylinder chamber updates are identical and independent within a substep, so they fit 4/8 lanes: an SoA layout with `wide::f64x4`/`f32x8`.
  - FV duct cells vectorise along the pipe for flux computation.
  - Valve/junction coupling is scalar glue.
- **Precision:** use f64 for the gas state. Mole counts and pressures span orders of magnitude, and engine-sim uses `double` throughout its gas system. f32 is fine for the audio post-chain.
- **Denormals:** set FTZ/DAZ once at physics-thread start and in the callback (`no_denormals` guard or `core::arch` MXCSR). Decaying resonator tails and filter states are the classic denormal sources. On aarch64 the equivalent is the FPCR FZ bit, which is not covered by the cited crate doc.
- **Determinism:** fixed dt, a fixed substep count (or one derived deterministically from state), no wall-clock inputs to physics, and a seeded RNG for noise make offline renders bit-reproducible on the same binary and CPU.
  - engine-sim's frame-time-driven step count (`dt * speed / timestep` from the UI framerate) is **not** deterministic.
  - Deterministic stepping is needed for regression tests and for the same engine rendering identically in live and export paths.
- **Live geometry/parameter changes without clicks:**
  - Build the new engine on the physics thread (or a worker), off the audio path.
  - Initialise it to a settled state. open-racing settles 1.5 s silently at idle, which can run faster than real time offline.
  - Then crossfade the two engines' outputs over ~20–50 ms, which runs both engines briefly.
  - Or, for continuous parameters, ramp values per substep (smoothing) rather than stepping them.
  - Drop the old engine off the real-time thread, e.g. via a return ring (the rtrb + non-RT-drain pattern).

### Gaps
- No measured xrun/underrun statistics or block-size recommendations from any engine simulator were found.
- No sourced per-step µs timings for 8–12 cylinders at 9000 rpm exist in the surveyed projects. The budget numbers above are arithmetic, not measurements.
- cpal default buffer sizes per backend (ALSA/PipeWire/WASAPI) were not verified in this pass.

---

## 4. Resampling and anti-aliasing from physics rate to 48 kHz

### Takeaway
engine-sim resamples its 10 kHz physics output to 44.1 kHz with plain **linear interpolation**. It adds a low-pass at 0.45 × fs and a per-channel low-pass at 1900 Hz, plus DC removal, a derivative filter, a jitter/noise stage, and an IR convolution of up to 10 000 samples. With a 10 kHz input, content above 5 kHz cannot be physical, and the "brightness" comes from post-processing. For higher fidelity:
- Either run the physics at an integer multiple of 48 kHz and decimate with a proper FIR/half-band filter.
- Or use a band-limited asynchronous sinc resampler (`rubato`) when the physics rate is not a clean ratio.

### Cited Findings
- engine-sim's synthesizer:
  - It advances `m_inputWriteOffset += audioSampleRate / inputSampleRate` and interpolates `last*(1-f) + x*f`, i.e. linear interpolation.
  - Filters: anti-aliasing low-pass at `audioSampleRate * 0.45`; a per-channel low-pass at 1900 Hz; DC filter at 10 Hz; a derivative filter; a jitter filter; an air-noise low-pass; convolution with an impulse response (up to 10 000 samples); a leveling filter.
  - The input sample rate tracks `simulationFrequency * simulationSpeed`.

  Source: [engine-sim src/synthesizer.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/synthesizer.cpp), [src/simulator.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/simulator.cpp)
- engine-sim's audio signal is built from `(exhaustRunnerAndPrimary.pressure − 1 atm)` plus dynamic-pressure terms scaled by 0.1, per cylinder. Each is delayed by (header primary + exhaust length) / 343 m/s through `m_delayFilters[i].initialize(delay, 10000.0)` — [engine-sim src/piston_engine_simulator.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/piston_engine_simulator.cpp)
- `rubato`:
  - Asynchronous resampling for non-locked, possibly varying ratios.
  - Its sinc resampler interpolates between oversampled points and has SIMD on x86_64 and aarch64.
  - It can run allocation-free in real time with pre-allocated buffers.
  - It offers fixed-input-chunk modes (`SincFixedIn`).

  Source: [docs.rs/rubato](https://docs.rs/rubato), [HEnquist/rubato](https://github.com/HEnquist/rubato/)
- `fft-convolver` is pure Rust uniform-partitioned FFT convolution with zero added latency and arbitrary block sizes, in f32/f64. This suits exhaust/body/room IRs in the callback — [neodsp/fft-convolver](https://github.com/neodsp/fft-convolver), [lib.rs/fft-convolver](https://lib.rs/crates/fft-convolver). Also available: `rt-fft-convolver` — [docs.rs](https://docs.rs/rt-fft-convolver/latest/rt_fft_convolver/)

### Inferences
- **Simplest good option:** run the physics at **96 kHz (2 × 48 k)**, or 48 kHz with gas substeps, and decimate 2:1 with a half-band FIR. The ratio is fixed and integer, so no asynchronous resampler is needed and it is deterministic.
  - The internal oversampling also serves as oversampling for the nonlinearities (choked flow, combustion pulses, waveguide steepening), which generate harmonics above Nyquist.
- **If the sim rate is arbitrary** (e.g. 10–20 kHz as in engine-sim, or crank-angle stepped): use `rubato` with the ratio fixed at start. For crank-angle stepping, the ratio varies per block (`set_resample_ratio_relative`). Linear interpolation from 10 kHz, as in engine-sim, images spectrum around multiples of 10 kHz. engine-sim hides this with the 1900 Hz per-channel low-pass.
- **Better than resampling the physics:** use a coarse physics rate for the 0D/FV low band, then synthesise the high band at 48 kHz with waveguides or delay lines driven by the coarse signal. This is the open-racing and EngineLab split, and engine-sim's delay-filter approach is a crude version. The high band then never passes through a resampler.
- Put the IR convolution and final filters in the callback or at the end of the physics thread. `fft-convolver` with a ~64–256 partition size is cheap relative to the gas model.

### Gaps
- No A/B listening data comparing linear vs sinc resampling in engine sound was found.
- Band-limited alternatives (BLEP-style correction for valve-opening impulses) are unexplored in the sources surveyed.

---

## 5. Survey of other physics-based engine sound simulators

### Takeaway
Open-source work splits into four tiers:
1. **engine-sim and its forks.** 0D gas volumes, lumped exhaust with delay lines, a 10 kHz rigid body with 8 gas substeps, and heavy post-filtering/IR. Widely judged convincing; many forks exist, but no Rust/WASM port was found.
2. **Newer quasi-1D duct simulators** (EngineLab, open-racing in Rust). FV + waveguide hybrids at 48 kHz output that are CPU-bound for V8/V12.
3. **Lightweight 0D-at-audio-rate toys** (ignis).
4. **Games** (BeamNG): sample-based. The physics drives rpm/load parameters, and the sound is pre-recorded samples with EQ and crossfades.

### Cited Findings
- **engine-sim (Ange Yaghi, C++):** a "combustion engine simulator that generates realistic audio" — [ange-yaghi/engine-sim](https://github.com/ange-yaghi/engine-sim).
  - Community forks: Open Engine Simulator (cross-platform build) — [zabayone/open-engine-sim](https://github.com/zabayone/open-engine-sim), [josemaarcos90-lgtm/open-engine-sim](https://github.com/josemaarcos90-lgtm/open-engine-sim). Also DDev247 — [DDev247/engine-sim](https://github.com/DDev247/engine-sim).
  - A WebAssembly/browser request (issue #428, June 2023) exists but no official port — [issue #428](https://github.com/ange-yaghi/engine-sim/issues/428).
- **engine-sim-community-edition:** a binary-only distribution of v0.1.14a ("there is no application code here"), marked no longer maintained and replaced by a newer commercial version — [Engine-Simulator/engine-sim-community-edition](https://github.com/Engine-Simulator/engine-sim-community-edition). The commercial Steam "Engine Simulator" exists — [Steam](https://store.steampowered.com/app/2381500/Engine_Simulator/), [engine-sim.parts](https://www.engine-sim.parts/).
- **EngineLab (zolaski333, C++20/JUCE):**
  - "Exhaust sound comes from simulated cylinder pressure through a physical duct network, not samples."
  - Features: ECU tuner, exhaust graph editor, a 16-engine catalogue, and measurement harnesses.
  - WAV export at 48/96/192 kHz.
  - Self-described as "not validated thermodynamic analysis software".
  - Exhaust elements: primaries, collectors, junctions, resonators, expansion chambers, porous packing, catalysts, and multiple outlets.

  Source: [zolaski333/EngineLab](https://github.com/zolaski333/EngineLab)
- **open-racing (r4ai, Rust):** the engine-sim module has MUSCL–Hancock ducts plus a waveguide layer:
  - Levine & Schwinger unflanged mouth radiation, Kirchhoff wall losses, and junction compliance.
  - Metghalchi & Keck laminar flame speed with turbulent flame S_T = S_L + √(S_L·u').
  - Afterfire, VTEC, turbo, and anti-lag.
  - Draft quality at 0.77× real time on a loaded machine.
  - This is the closest found Rust precedent.

  Source: [r4ai/open-racing PR #13](https://github.com/r4ai/open-racing/pull/13)
- **ignis (C++20/SDL2):**
  - 0D per-cylinder gas at audio rate with Wiebe burn and a sequential-impulse crank solver.
  - Audio is DC-blocked pressure-derivative plus delay-line pipe resonances.
  - Explicitly lacks 1D exhaust acoustics and is limited to ≤ 12 cylinders.

  Source: [xevrion/ignis](https://github.com/xevrion/ignis)
- **Web demos:** realenginesimulator.com synthesises exhaust from firing order in Web Audio, with Wiebe heat release on Otto-cycle thermodynamics and crank-slider kinematics, and torque from computed cylinder pressure every cycle — [realenginesimulator.com](https://realenginesimulator.com/). Also "Engine Lab – Modular Engine & Fuel Physics Simulator" — [enginelab.netlify.app](https://enginelab.netlify.app/). Neither was inspected in depth.
- **BeamNG.drive:** engine audio is **sample-based**, mixed at runtime from two continuous layers:
  - `soundConfig`: mechanical, induction, and engine-bay sound.
  - `soundConfigExhaust`: outlet sound.

  They are modulated by physics-derived rpm and load through `rpmSmootherInRate/OutRate`, `onLoadGain`/`offLoadGain`, `maxLoadMix`/`minLoadMix`, and `loadSmootherIn/OutRate`. There is engine-order EQ from `fundamentalFrequencyCylinderCount` and `eqFundamentalGain`, parametric EQ, and muffling coefficients. Starter, shutoff, afterfire, turbo, transmission, wind, and tyre sounds are layered around them. The docs do not state the physics-to-audio update relationship — [BeamNG Engine Sound Tuning docs](https://documentation.beamng.com/modding/vehicle/sections/sounds/engine_audio/)
- BeamNG community tools such as EngineSynth build engine sounds by sequencing percussive samples by cylinder count and firing order, then render them offline for BeamNG — [Maxsimuss/EngineSynth](https://github.com/Maxsimuss/EngineSynth)
- **Research:** Baldan, Lachambre, Delle Monache, Boussard, "Physically informed car engine sound synthesis for virtual and augmented environments" (IEEE VR SIVE workshop 2015).
  - Procedural, physically informed, and "computationally efficient alternative to sample-based and analysis/resynthesis approaches".
  - Implemented as a Max/MSP external in the GeneCars driving simulator and SkAT Studio.
  - Cylinders are waveguides fed by piston motion and ignition pressure.
  - Exhaust collectors are fixed-length waveguides with valve-end feedback modulated by the exhaust valve.

  Sources: [ResearchGate](https://www.researchgate.net/publication/280086598_Physically_informed_car_engine_sound_synthesis_for_virtual_and_augmented_environments), [academia.edu](https://www.academia.edu/14107512/Physically_informed_car_engine_sound_synthesis_for_virtual_and_augmented_environments). The IUAV PDF returned 403.
- **Newer ML/hybrid research:** "Real-Time Automotive Engine Sound Simulation with Deep Neural Network" (NCMMSC 2023) is a hybrid sample-based + procedural approach driven by rpm and pedal data — [Springer](https://link.springer.com/chapter/10.1007/978-981-97-0601-3_15), [Duke PDF](https://sites.duke.edu/dkusmiip/files/2023/12/Engine_Sound_Synthesis_ncmmsc-52-1.pdf)

### Inferences
- **Games decouple on purpose.** BeamNG's physics produces rpm/load, and the sound is samples plus EQ. rFactor 2 and Assetto Corsa are commonly described the same way, but no primary source was fetched for them (see Gaps).
- **The physically simulated tier sounds alive for these reasons:**
  1. Cycle-to-cycle variation emerges on its own rather than being scripted: EngineLab explicitly claims "emergent cycle-to-cycle variability".
  2. Firing-order and exhaust-geometry effects come for free.
  3. Transients (throttle blips, afterfire) are consistent with the physics.
- **Practical recipe from the survey:**
  - 0D cylinders.
  - Positivity-clamped explicit valve flows (engine-sim style).
  - A low-band FV duct or lumped volumes.
  - A waveguide/delay-line high band at 48 kHz.
  - Radiation filter + IR convolution.
  - Physics on its own thread behind a wait-free ring.

  engine-sim's quality with only a lumped exhaust shows the audio post-chain (derivative, IR convolution, noise) carries much of the perceived realism.

### Gaps
- No Rust or WebAssembly port of engine-sim itself was found. The closest Rust work is open-racing's independent reimplementation.
- rFactor 2 / Assetto Corsa physics-vs-sound decoupling: no primary sources (developer docs or talks) were retrieved in this pass.
- AMESim/GT-Power-to-sound research (offline 1D codes used for sound): no primary paper was retrieved.
- There is no objective quality comparison (listening tests) across engine-sim, EngineLab, and open-racing. Quality judgements in the sources are the authors' own.

---

## 6. Rust ecosystem

### Takeaway
Everything needed is available and allocation-free-capable:
- `rtrb` for wait-free SPSC between physics, callback, and UI.
- `rubato` for band-limited asynchronous resampling.
- `fft-convolver` for partitioned IR convolution.
- `no_denormals` for FTZ/DAZ.
- `wide` (stable) or `std::simd` (nightly) for SIMD.

A general ODE crate is not recommended in the hot loop. Hand-written fixed-step integrators over SoA arrays are what every surveyed project uses.

### Cited Findings
- rtrb is a wait-free, lock-free SPSC ring buffer intended for real-time audio — [mgeier/rtrb](https://github.com/mgeier/rtrb), [rust.audio forum](https://rust-audio.discourse.group/t/announcement-real-time-ring-buffer-rtrb/346)
- rubato: async sinc resampler with SIMD (x86_64, aarch64), real-time use with pre-allocated buffers — [docs.rs/rubato](https://docs.rs/rubato)
- fft-convolver: uniform and non-uniform partitioned FFT convolution, zero latency, f32/f64 — [neodsp/fft-convolver](https://github.com/neodsp/fft-convolver)
- no_denormals: an RAII FTZ/DAZ guard (x86/x86_64 MXCSR) — [docs.rs/no_denormals](https://docs.rs/no_denormals)
- The surveyed simulators all use hand-written integrators:
  - engine-sim: explicit first-order gas, NSV rigid-body solver ([gas_system.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/gas_system.cpp)).
  - ignis: a custom sequential-impulse solver ([ignis](https://github.com/xevrion/ignis)).
  - open-racing: custom MUSCL–Hancock ([PR #13](https://github.com/r4ai/open-racing/pull/13)).

### Inferences
- **ODE crates** (e.g. `ode_solvers`, `diffsol`) are designed for adaptive-step offline integration with dynamic dispatch. For a fixed-step, real-time, positivity-clamped scheme over a few hundred states they add overhead and allocation risk with no benefit.
- `realfft`/`rustfft` are only needed directly for offline spectral analysis. `fft-convolver` already wraps FFT convolution.
- `ringbuf` is an alternative SPSC crate. rtrb is the one designed and advertised specifically for real-time audio.

### Gaps
- No Rust crate dedicated to audio-rate physical modelling of gas dynamics or waveguides was found. It would be built by hand.
- The `wide` and `std::simd` stabilisation status as of 2026 was not verified from a primary source in this pass.
