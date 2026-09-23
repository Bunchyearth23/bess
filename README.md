# BESS — Bunchy's Engine Synthesis System

BESS is a Windows application that sits between **Automation** and **BeamNG.drive**. It takes the engine sounds in an Automation vehicle export, lets you shape and preview them, and adds a selectable BESS sound configuration to the original vehicle in BeamNG.

The main workflow is:

1. **Import from Automation:** Open a vehicle ZIP. BESS reads its engine WAV bank across RPM and load while keeping the original sounds available as a reference.
2. **Resynthesize in BESS:** Use the imported recording to guide engine-cycle timing and sound character, then adjust pulses, dynamics, inferred intake detail, exhaust acoustics and texture. Compare the source with BESS in live playback, save a project or render WAVs.
3. **Export for BeamNG.drive:** Create a small configuration add-on ZIP. Keep the original Automation vehicle mod enabled, then select its **(BESS)** configuration in BeamNG. The original configuration and vehicle physics remain available; listen in game to validate the result.

BESS is built with Rust, egui and BDSP. The Automation WAV bank supplies the timing and sound identity for source-guided resynthesis. Automation exports only exhaust audio, so BESS's engine-side and intake detail are estimates, not recovered recordings.

## Get started

1. Download the [Windows release](https://github.com/Bunchyearth23/bess/releases/tag/v0.9.0), extract the portable ZIP, open `START.html`, launch **BESS.exe**, and choose **Import Automation ZIP**. A standalone executable is also available on the release page.
2. Wait for both load banks to finish loading. Choose **Play** or press Space.
3. Compare **A · Automation source** with **B · BESS resynthesis**. Level compensation helps compare tone in the live mix. A replays prepared WAV loops; it is not a capture of Automation's in-game sound engine. The optional **BeamNG two-emitter preview** follows the export's exhaust level calibration and engine balance in real time, then approximates a mono mix using a generic -8 dB engine-to-exhaust gain. BeamNG's spatial and cabin processing can sound different.
4. Use the **Driving** block at the upper right; its top button shows or hides it. In **Simulated driving**, apply throttle, select an automatic gearbox or manual gears N/1–6, and add braking or resistance at the wheels. **Reset to standstill** resets the vehicle; stopping playback pauses it.
5. Adjust dynamics, pulses, intake, exhaust acoustics, mechanical texture and optional turbo. Added effects apply only to B.
6. Try **Balanced**, **Muted** and **Open**. These presets change the character of the same engine while preserving rpm, load, volume and driving response.

B uses the imported recording to guide a new engine-cycle excitation and acoustic response. **Cycle variation**, **Pressure front**, **Pulse-linked texture** and **Resynthesis amount** shape that treatment. A remains the original prepared reference. The loop and neighboring RPM points share a cycle timebase to reduce the level pumping previously heard around 5,200 rpm on the Cerberus. These controls are sound models, not measurements of cylinder pressure or intake flow.

The acoustic path has a header, expansion chamber and outlet. Length, diameter, volume, absorption and temperature change delays and reflections. The intake has a separate duct, but its excitation is inferred from exhaust-only audio. Mechanical detail is modeled rather than measured at the engine bay. Deceleration fuel cut attenuates combustion; lift-off pops excite the exhaust. Scroll the left settings panel to reach the intake and save controls.

BESS uses the default Windows audio device. It prefers a 48 kHz floating-point output when available, then another compatible 48 kHz format, and otherwise falls back to the device default. The actual format is shown in the application; internal processing stays in floating point. Exported WAVs remain **48 kHz / 24-bit PCM**. **Reconnect audio** applies a device change. Imports and renders run in the background.

## 0.9.0 delivery

The portable v0.9.0 release contains the Windows application and its instructions. It does not include test vehicles, sound banks, sample projects, or prepared BeamNG add-ons. Import your own Automation vehicle ZIP into BESS, then export a matching BESS configuration add-on. Keep your original vehicle ZIP enabled beside that add-on in BeamNG. Select the trim marked **(BESS)**; the original trim remains available.

The source-guided resynthesis and optional two-emitter listening preview described in this README are current development work after v0.9.0. The public v0.9.0 downloads contain the earlier audio model.

In **Engine and combustion**, enable a configuration only when the engine data are known: cylinder count, angles over 720°, force, pressure duration and exhaust-opening delay. The initial evenly spaced angles are not a manufacturer's firing order. These additions are optional and also controlled by **Resynthesis amount**. If an Automation `.car` sheet matches the audio blend, active engine and JBeam, BESS shows its declared cylinder count and layout. It does not infer firing order or bank phasing. Optional activation suggests the declared count when available.

The current GUI's BeamNG export creates an add-on ZIP containing a new configuration for the imported Automation vehicle. Keep the original vehicle ZIP enabled and add the BESS ZIP alongside it. In the vehicle selector, choose the original vehicle and then its configuration named after the original trim with **(BESS)** appended, such as **A (BESS)** for Cerberus A. The add-on supplies a uniquely named engine part, separate engine/intake and exhaust sound blends, and mono 48 kHz / 24-bit PCM WAVs for both, plus a configuration file and its display metadata. Both sound banks are derived from the Automation recording and BESS's resynthesis. The engine-side sound is an estimate made from exhaust-only input, not an isolated engine-bay or intake recording. The add-on does not overwrite the original vehicle's `info.json`, configuration or audio. The original trim stays selectable.

The add-on changes the selected engine sound, not vehicle physics. BeamNG mixes the engine-side layer near the engine bay with the exhaust layer at the tailpipe. Start, stop, pops and turbo references remain those of the vehicle. BESS driving transients are not exported as an in-game controller. Playback volume and A/B compensation are ignored. At each RPM/load point, the exported exhaust aims for 1 dB below the original WAV's AC RMS (DC offset removed), with bounded gain and a peak ceiling; a final common safety gain remains available. Exported RPM knots start at a common crank phase. The engine-side loops last about four seconds, while exhaust loops remain about two seconds; both ultimately derive from short source recordings. Engine gain is bounded at each knot so an inferred detail cannot dominate one part of the sweep. The two original load layers remain, so BeamNG's intermediate interpolation does not reproduce every internal BESS curve exactly. **BESS live mix** plays the local synthesis; **BeamNG two-emitter preview** follows those file-level gain rules with smoothed real-time estimates and approximates the two emitters in mono with a generic -8 dB engine-to-exhaust gain. It does not reproduce the exact per-knot offline statistics, the game's spatial mix or cabin filtering. Compare the original and BESS trims from the cockpit, hood, and tailpipe cameras. In-game listening validation is still required. The older full-replacement export remains available through the legacy API; it requires disabling the original mod because its internal vehicle paths overlap.

Before exporting, use **Calculate BeamNG level** in the BeamNG export panel. BESS renders the full RPM/load sample bank in the background, then shows the BESS exhaust WAV's AC level change against the original Automation WAV and the new engine WAV's level relative to the BESS exhaust. **Inspect at RPM** moves through the range without recalculating and shows the neighboring export points; off-load and full-load rows remain separate. Expand **WAV level details** for AC RMS and peak values in dBFS. Changing an export-affecting sound setting makes the analysis stale until recalculated, while moving the inspection RPM or changing listening volume does not. These measurements describe the files sent to BeamNG. Camera position, cabin filtering, vehicle exhaust parts and BeamNG's mix still determine the audible volume in the game.

## Settings and comparisons

The 0.9.0 portable package contains no example vehicles or listening corpus. Use your own Automation exports for A/B listening and BeamNG configurations. Measurements describe signal differences; only listening can judge naturalness.

Hover over a slider and turn the **mouse wheel** to adjust it; **Shift + wheel** makes finer adjustments. Elsewhere, the wheel scrolls the panel.

**At high rpm** and **At full load** replace the editing grids. Negative values soften and positive values emphasize the sound. They affect pulses, texture, intake and exhaust differently. Old maps remain active until these controls are changed, and the replacement is indicated.

**Resynthesis amount** controls how strongly the source-guided engine model contributes. New imports start with settings adapted to the WAV measurements, without assuming pops or turbo. Processing cannot recover details absent from the Automation files. For an existing project, **Adapt to vehicle / natural base** resets the hybrid settings to that basis while preserving A/B selection and level compensation; it replaces manual settings. Consolidated work is tracked in `docs/STEPLIST.md`.

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

**Export A/B comparison** creates two equal-RMS 16-second WAVs in a new subfolder; equal RMS does not mean equal LUFS loudness. **Export three characters + source** adds four equal-RMS clips and reopenable projects. Those projects preserve settings from before final comparison-WAV level matching. Both comparison exports always use the reference cycle.

Requested rpm is clamped to the minimum and maximum of the exported sounds, including typed input, restored projects, automatic cycles and WAV rendering. The original reference ZIP has 28 rpm points × 2 loads from 803 to 4,989 rpm. Its 12,000 rpm JBeam damage threshold does not extend the sound bank.

## Development commands

- `cargo run --release -- --open vehicle.zip`: open and import a vehicle.
- `cargo run --release -- --compare vehicle.zip folder`: 16-second comparison.
- `cargo run --release -- --beamng vehicle.zip new-folder`: configuration add-on with calibrated BESS loops; keep the original mod enabled.
- `cargo run --release -- --beamng-replacement vehicle.zip new-folder`: legacy full copy; disable the original mod while using it.
- `cargo run --release --example combustion_demo -- vehicle.zip new-folder`: three demos and explicitly configured combustion projects.
- `cargo run --release -- --characters vehicle.zip folder`: source and three characters.
- `cargo run --release -- --drive-demo vehicle.zip folder`: open road, 900 Nm load and neutral; WAV, projects and telemetry CSV over 16 seconds.
- `cargo run --release -- --audio-check report.txt vehicle.zip 30`: real computation into silent output for 30 seconds (default five), counting callbacks and overruns.
- `cargo test --release` and `cargo clippy --all-targets -- -D warnings`.
- `graft build` then `graft check`: deterministic local context graph.
- `--render file.wav` retains the synthetic MVP 0.1 diagnostic.
- `--capture image.png vehicle.zip`: egui capture after import for QA.

BDSP is pinned to `b2981d431d321d29224752a2f855495da965d4bb`. The BDSP repository is currently private, so building this public source checkout requires access to it, Rust/MSVC and GitHub SSH. The Windows executable in the release asset runs without repository access. `Cargo.lock` pins dependencies.

## Current scope

The imported bank provides the seed and reference for resynthesis; BESS does not capture a new engine. Intake and mechanical layers are inferred rather than physically isolated from the exhaust recording. Acoustics uses a reduced pressure-wave network with losses and section changes. Its dimensions are sound controls, not extracted vehicle dimensions. Wave speed depends on an effective temperature with an ideal-gas approximation; there is no thermodynamic or aerodynamic simulation. Source recordings may already contain exhaust coloration, which BESS cannot fully separate or remove.

Profile alignment is an initial loop preparation, not the full PSOLA algorithm described in the research. A file with multiple blends is refused until explicit blend selection is available. Per-bank headers and automatic part identification are outside the current model. Explicit combustion remains a phenomenological excitation without thermodynamic pressure calculation. Driving-history effects require more than static exported loops. Listening and real BeamNG operation still need validation.

[Research and roadmap](https://github.com/Bunchyearth23/bess/blob/main/docs/ENGINE-SOUND-RESEARCH.md) · [Project index](https://github.com/Bunchyearth23/bess/blob/main/docs/INDEX.md).
