# BESS — Bunchy's Engine Synthesis System

BESS is a Windows application that sits between **Automation** and **BeamNG.drive**. It takes the engine sounds in an Automation vehicle export, lets you shape and preview them, and adds a selectable BESS sound configuration to the original vehicle in BeamNG.

The main workflow is:

1. **Import from Automation:** Open a vehicle ZIP. BESS reads its engine WAV bank across RPM and load while keeping the original sounds available as a reference.
2. **Shape sound in BESS:** New imports start with source-guided resynthesis. Set how much Automation timbre to retain, balance the exhaust, intake and mechanical layers, and compare B with the original A reference. Open the advanced controls for pulse, texture and acoustic geometry. The separate experimental listening mode remains available in the interface.
3. **Export for BeamNG.drive:** Give the sound profile a name and create a configuration add-on ZIP using standard resynthesis, regardless of the experimental listening switch. Keep the original Automation vehicle mod enabled, then select its **(BESS - Profile)** configuration in BeamNG. Several named profiles can coexist with the original configuration and vehicle physics.

BESS is built with Rust, egui and BDSP. Its standard BeamNG sound uses Automation's exhaust recording and measured descriptors as adjustable excitation for the BESS pulse, texture and acoustic paths. At **0% Automation timbre**, the standard sound uses descriptor-derived excitation without replaying the original waveform; at **100%**, it retains the recorded timbre while BESS effects still act on it. The optional experimental listening mode remains separate. Automation exports only exhaust audio, so engine-side and intake detail remain estimates rather than recovered recordings.

Banks with an extreme recorded idle-level jump receive a bounded B-only RPM balance in both listening modes. See the [b5_a idle correction](docs/reports/BESS-B5-A-IDLE-BALANCE-2026-09-23.md) and its unnormalized local listening clips.

## Get started

1. Use a Windows package for this version, extract the portable ZIP, open `START.html`, launch **BESS.exe**, and choose **Import Automation ZIP**. The project also provides a standalone executable and the application-only packaging commands below. See [published releases](https://github.com/Bunchyearth23/bess/releases) for available downloads.
2. Wait for both load banks to finish loading. Choose **Play** or press Space.
3. Compare **A · Automation source** with **B · BESS resynthesis**. Level compensation helps compare tone in the live mix. A replays prepared WAV loops; it is not a capture of Automation's in-game sound engine. The **BeamNG camera preview** (**Cockpit / Interior**, **Hood**, **Tailpipe**, **Orbit / Exterior**) lets you audition in-bench acoustic perspectives with cabin low-pass filtering and firewall attenuation.
4. Use the **Driving** block at the upper right; its top button shows or hides it. In **Simulated driving**, apply throttle, select an automatic gearbox or manual gears N/1–6, and add braking or resistance at the wheels. **Reset to standstill** resets the vehicle; stopping playback pauses it.
5. Set **Automation timbre retained**, **Resynthesis amount**, and the exhaust, intake and mechanical levels. Use **Advanced sound controls** for dynamics, pulses, texture, acoustic geometry and optional turbo. Added effects apply only to B.
6. Try **Balanced**, **Muted**, **Open**, **Warm**, **Mechanical** and **Grit**. These presets change tone while preserving engine timing and driving response. The generated body, edge, flow and mechanical controls apply only when experimental listening is enabled.

B uses the source-guided engine cycle and acoustic response by default. The retained-timbre control blends the recorded pressure and texture with new excitation built from the ZIP's measured level, engine orders and broad spectral bands. **Cycle variation**, **Pressure front**, **Pulse-linked texture** and **Resynthesis amount** shape both inputs. A remains the prepared Automation reference. The optional experimental mode generates a separate B waveform from descriptors measured from the bank. BeamNG export always uses the standard B sound. Neither mode measures cylinder pressure or intake flow.

Name each exported sound in **Sound profile name**. Exports with different names have distinct configuration, engine part, blend and WAV paths, so Natural, Smooth and Raw can be tested side by side on one Automation vehicle. Save the project to retain both its sound settings and profile name. A name must be 1–48 characters and contain no slash or control character.

The current experimental development preview measures audible bass and upper texture separately at each RPM/load point, then generates new pressure pulses with slight event variation and independent flow and mechanical texture. Its sound can differ from saved experimental projects made with earlier builds. A repeatable local twelve-vehicle audit is described in the [experimental fleet report](docs/reports/BESS-EXPERIMENTAL-FLEET-2026-09-23.md); the generated comparison WAVs and vehicle ZIPs are not part of the public repository.

The acoustic path has a header, expansion chamber and outlet. Length, diameter, volume, absorption and temperature change delays and reflections. The intake has a separate duct, but its excitation is inferred from exhaust-only audio. Mechanical detail is modeled rather than measured at the engine bay. Deceleration fuel cut attenuates combustion; lift-off pops excite the exhaust. Scroll the left settings panel to reach the intake and save controls.

BESS uses the default Windows audio device. It prefers a 48 kHz floating-point output when available, then another compatible 48 kHz format, and otherwise falls back to the device default. The actual format is shown in the application; internal processing stays in floating point. Exported WAVs remain **48 kHz / 24-bit PCM**. **Reconnect audio** applies a device change. Imports and renders run in the background.

## 0.11.1 delivery

The portable v0.11.1 application package contains the Windows application and its instructions. It does not include test vehicles, sound banks, sample projects, or prepared BeamNG add-ons. Import your own Automation vehicle ZIP into BESS, then export a matching BESS configuration add-on. Keep your original vehicle ZIP enabled beside that add-on in BeamNG. Select the trim marked **(BESS)**; the original trim remains available.

A separate recording-free instrument package contains `standalone_engine.exe`, `standalone_live.exe`, three generic JSON presets and a quick-start guide. It does not need an Automation ZIP. The live instrument uses a terminal and the default Windows audio device; it is not integrated into the main BESS GUI or BeamNG export.

New imports use the standard source-guided mode. The experimental generated mode remains available for live listening and comparison; the BeamNG add-on, two-emitter preview and level panel always use the standard mode. Existing projects keep their saved mode. Final loudness and naturalness need in-game listening.

In **Engine and combustion**, enable a configuration only when the engine data are known: cylinder count, angles over 720°, force, pressure duration and exhaust-opening delay. The initial evenly spaced angles are not a manufacturer's firing order. These additions are optional and also controlled by **Resynthesis amount**. If an Automation `.car` sheet matches the audio blend, active engine and JBeam, BESS shows its declared cylinder count and layout. It does not infer firing order or bank phasing. Optional activation suggests the declared count when available.

The current GUI's BeamNG export creates an add-on ZIP containing a new named configuration for the imported Automation vehicle. Keep the original vehicle ZIP enabled and add each BESS ZIP alongside it. In the vehicle selector, choose the original vehicle and then a configuration such as **A (BESS - Natural)** for Cerberus A. The add-on supplies a uniquely named engine part, separate engine/intake and exhaust sound blends, and mono 48 kHz / 24-bit PCM WAVs for both, plus a configuration file and its display metadata. Both sound banks are guided by measurements of the Automation recording and BESS's resynthesis. The engine-side sound is an estimate made from exhaust-only input, not an isolated engine-bay or intake recording. The add-on does not overwrite the original vehicle's `info.json`, configuration or audio. The original trim stays selectable.

The add-on changes the selected engine sound, not vehicle physics. BeamNG mixes the engine-side layer near the engine bay with the exhaust layer at the tailpipe. Start, stop, pops and turbo references remain those of the vehicle. BESS driving transients are not exported as an in-game controller. Playback volume and A/B compensation are ignored. At each RPM/load point, the exported exhaust aims for 1 dB below the original WAV's AC RMS (DC offset removed), with bounded gain and a peak ceiling; a final common safety gain remains available. Exported RPM knots start at a common crank phase. The engine-side loops last about four seconds, while exhaust loops remain about two seconds; their sound is resynthesized from Automation-derived timing and texture. Engine gain is bounded at each knot so an inferred detail cannot dominate one part of the sweep. The two original load layers remain, so BeamNG's intermediate interpolation does not reproduce every internal BESS curve exactly. **BESS live mix** plays the local synthesis; **BeamNG two-emitter preview** follows those file-level gain rules with smoothed real-time estimates and approximates the two emitters in mono with a generic -8 dB engine-to-exhaust gain. It does not reproduce the exact per-knot offline statistics, the game's spatial mix or cabin filtering. Compare the original and BESS trims from the cockpit, hood, and tailpipe cameras. In-game listening validation is still required. The older full-replacement export remains available through the legacy API; it requires disabling the original mod because its internal vehicle paths overlap.

Before exporting, use **Calculate BeamNG level** in the BeamNG export panel. BESS renders the full RPM/load sample bank in the background, then shows the BESS exhaust WAV's AC level change against the original Automation WAV and the new engine WAV's level relative to the BESS exhaust. **Inspect at RPM** moves through the range without recalculating and shows the neighboring export points; off-load and full-load rows remain separate. Expand **WAV level details** for AC RMS and peak values in dBFS. Changing an export-affecting sound setting makes the analysis stale until recalculated, while moving the inspection RPM or changing listening volume does not. These measurements describe the files sent to BeamNG. Camera position, cabin filtering, vehicle exhaust parts and BeamNG's mix still determine the audible volume in the game.

## Settings and comparisons

The 0.11.1 portable application contains no example vehicles or listening corpus. Use your own Automation exports for A/B listening and BeamNG configurations. Measurements describe signal differences; only listening can judge naturalness.

Hover over a slider and turn the **mouse wheel** to adjust it; **Shift + wheel** makes finer adjustments. Elsewhere, the wheel scrolls the panel.

**At high rpm** and **At full load** replace the editing grids. Negative values soften and positive values emphasize the sound. They affect pulses, texture, intake and exhaust differently. Old maps remain active until these controls are changed, and the replacement is indicated.

**Resynthesis amount** controls the standard source-guided model. New imports start in that mode with settings adapted to the WAV bank, without assuming pops or turbo. Processing cannot recover details absent from the Automation files. For an existing project, **Adapt to vehicle / natural base** resets sound settings to the imported vehicle's basis while preserving the selected sound mode, A/B selection and level compensation; it replaces manual settings. Consolidated work is tracked in `docs/STEPLIST.md`.

**Source pulses** and **Natural source texture** are approximate components of the imported recording. They do not identify individual cylinders or isolate intake and mechanical microphones. Uncertain periods keep their full signal in the texture.

## Driving and export

Driving controls are in the upper-right block, shown at startup and toggled by **Driving**. The block also has play/pause and rpm, load, speed and gear feedback. The left **Sound settings** panel covers the source and acoustic character. The waveform spans the width below both blocks. In a narrow window, the blocks stack vertically.

Driving supplies rpm and load to synthesis, so it changes rendered WAVs. It does not alter BeamNG vehicle physics.

- **Simulated driving:** throttle, brake, automatic or manual gearbox, resistance in Nm after gearbox and final drive, and uphill grade. Sound rpm and load result from the engine, clutch and vehicle acceleration. Speed, engaged gear, wheel torque and total resistance are displayed.
- **Direct rpm / load:** the earlier bench's two direct controls.
- **Comparison cycle:** a 16-second idle, acceleration, lift-off and recovery sequence.

**Vehicle and bench gears** sets mass, peak torque, inertia, wheel radius, final drive and six gears. These values are generic and are not identified from the ZIP. The model is limited to forward motion, with an automatic launch clutch, stall prevention and a ceiling at the top of the sound bank. An out-of-range downshift waits for compatible speed.

## Projects and renders

Projects saved by the interface use version 3 with driving settings. Versions 1 and 2 remain readable and default to direct mode when driving settings are absent. Projects also store a SHA256 reference to the bank. The WAVs remain inside the ZIP, so keep that file in place. BESS refuses to restore a changed bank; reimport it explicitly. Version 1 projects require an import for hybrid sound. Projects from 0.2 remain readable with defaults for new settings, but the 0.3 acoustic model changes their sound; the 0.2 executable remains available.

**Export selected mode** creates a mono **48 kHz / 24-bit** WAV:

- Simulated driving starts from standstill with current throttle, brake and load held, using the selected gearbox. It does not record past interactions.
- Direct mode holds requested rpm and load.
- Comparison cycle renders the complete sequence adapted to the selected duration.

**Export A/B comparison** creates two equal-RMS 16-second WAVs in a new subfolder; equal RMS does not mean equal LUFS loudness. **Export six characters + source** adds seven equal-RMS clips and reopenable projects. Those projects preserve settings from before final comparison-WAV level matching. Both comparison exports always use the reference cycle.

Requested rpm is clamped to the minimum and maximum of the exported sounds, including typed input, restored projects, automatic cycles and WAV rendering. The original reference ZIP has 28 rpm points × 2 loads from 803 to 4,989 rpm. Its 12,000 rpm JBeam damage threshold does not extend the sound bank.

## Standalone procedural instrument (no Automation ZIP required)

Download `BESS-0.11.0-Standalone-Windows-Portable.zip` from the 0.11.0 release to use this separate instrument without a source checkout. Extract it, then see `START_STANDALONE.md` for the live and WAV commands.

The revised noise calibration and BESS Intake/Mechanics correction are described in the [noise correction report](docs/reports/BESS-NOISE-CORRECTION-2026-09-23.md). Fresh standalone listening clips are in `output/standalone-noise-fix-20260923/` locally.

This separate four-stroke prototype generates sound from a versioned JSON engine configuration and external rpm/load commands. It does not replace the standard Automation-guided BeamNG export. The attached [standalone mission](docs/standalone-mission.md), [design](docs/design.md), [references](docs/references.md) and [validation](docs/validation.md) describe its scope and evidence.

```powershell
cargo build --release --bin standalone_engine --bin standalone_live
cargo run --release --bin standalone_engine -- --write-presets presets/standalone
cargo run --release --bin standalone_engine -- --config presets/standalone/four-even.json --scenario steady --seconds 4 --out output/steady.wav
cargo run --release --bin standalone_engine -- --compare output/preset-comparison --scenario steady --seconds 4
cargo run --release --bin standalone_engine -- --preset four-split --scenario ramp --seconds 4 --out output/ramp.wav
cargo run --release --bin standalone_engine -- --preset four-split --scenario ramp --seconds 10 --benchmark
cargo run --release --bin standalone_live -- --config presets/standalone/four-split.json
```

The offline scenarios are `steady`, `ramp`, `load` and `shutdown`; output is mono PCM24 at 48 kHz by default, with `--rate` available from 8–192 kHz. `--rpm`, `--load` and `--volume` set initial or held commands. `--compare` renders all three presets with the same commands and no level matching. In the live terminal, enter `rpm 3000`, `load 0.5`, `volume 0.6`, `exhaust 0.7`, `intake 0.3`, `block 0.2`, `combustion fuel_cut`, or `quit`. These are sound and phase controls, not a vehicle torque model. The output device may use two channels carrying the same mono sample. The three presets are illustrative, and source levels are not independently normalized.

## Development commands

- `cargo run --release -- --open vehicle.zip`: open and import a vehicle.
- `cargo run --release -- --compare vehicle.zip folder`: 16-second comparison.
- `cargo run --release -- --beamng vehicle.zip new-folder`: configuration add-on with standard source-guided BESS loops; keep the original mod enabled.
- `cargo run --release -- --beamng-replacement vehicle.zip new-folder`: legacy full copy; disable the original mod while using it.
- `cargo run --release --example combustion_demo -- vehicle.zip new-folder`: three demos and explicitly configured combustion projects.
- `cargo run --release -- --characters vehicle.zip folder`: source and six characters.
- `cargo run --release -- --drive-demo vehicle.zip folder`: open road, 900 Nm load and neutral; WAV, projects and telemetry CSV over 16 seconds.
- `cargo run --release -- --audio-check report.txt vehicle.zip 30`: real computation into silent output for 30 seconds (default five), counting callbacks and overruns.
- `cargo test --release` and `cargo clippy --all-targets -- -D warnings`.
- `graft build` then `graft check`: deterministic local context graph.
- `--render file.wav` retains the synthetic MVP 0.1 diagnostic.
- `--capture image.png vehicle.zip`: egui capture after import for QA.

BDSP is pinned to `b2981d431d321d29224752a2f855495da965d4bb`. The BDSP repository is currently private, so building this public source checkout requires access to it, Rust/MSVC and GitHub SSH. The Windows executable in the release asset runs without repository access. `Cargo.lock` pins dependencies.

## Current scope

The imported bank provides a reference and measured descriptors; BESS does not capture a new engine. The experimental generated waveform is independent of source PCM, but it is still an estimate and is not used for BeamNG export. Intake and mechanical layers are inferred rather than physically isolated from the exhaust recording. The standard source-guided acoustic path uses a reduced pressure-wave network with losses and section changes. Its dimensions are sound controls, not extracted vehicle dimensions. There is no thermodynamic or aerodynamic simulation. Source recordings may already contain exhaust coloration, which BESS cannot fully separate or remove.

Profile alignment is an initial loop preparation, not the full PSOLA algorithm described in the research. A file with multiple blends is refused until explicit blend selection is available. Per-bank headers and automatic part identification are outside the current model. Explicit combustion remains a phenomenological excitation without thermodynamic pressure calculation. Driving-history effects require more than static exported loops. Listening and real BeamNG operation still need validation.

[Research and roadmap](https://github.com/Bunchyearth23/bess/blob/main/docs/ENGINE-SOUND-RESEARCH.md) · [Project index](https://github.com/Bunchyearth23/bess/blob/main/docs/INDEX.md).
