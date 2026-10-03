# BESS — Bunchy's Engine Synthesis System

BESS is a Windows engine sound workshop built with Rust, egui and BDSP. It reads an Automation vehicle ZIP, builds a physical engine from its verified engine data, and exports a selectable sound configuration for BeamNG.drive. The same physical engine also powers engines designed from scratch.

There are two listening choices:

- **A · Source Automation** plays the original imported WAV bank across RPM and load. It receives no B sound shaping. Playback interpolation, listening volume and output safety still apply; this is not a recording of the Automation game.
- **B · BESS physical engine** generates cylinder pressure, valve flow, intake, exhaust and mechanical sound. The original waveform never enters B. The same engine supplies live listening, WAV renders and BeamNG exports.

## Use the application

1. Launch `BESS.exe` and choose **Import Automation ZIP…**, or **New engine from scratch**.
2. For an import, check the engine identity and the visible model assumptions. Cylinder layout and capacity must be verified against the selected sound bank and active vehicle engine. Missing required data leave A available and make B unavailable with an explanation.
3. Compare A and B. Set exhaust, intake and mechanical levels, engine layer gain and idle gain. New engines and presets start with the mechanical level at zero; saved levels remain editable. Optional level matching helps compare tone in the live mix.
4. Shape the physical sound with 29 saved controls: shared tone and response, intake airflow/resonance/length, mechanical pitch/resonance, combustion and exhaust propagation. **Exhaust sound**, before the engine parts, groups **Exhaust decay (ms)** and **Experimental back pressure**. The decay control shortens or lengthens the internal reflections independently of Room; try 40 ms for a drier exhaust, or 120 ms for the previous setting. It works without a muffler and is saved for listening and exports. **Exhaust tone** adds separate bass, body gain/frequency/width, rasp, low/high cuts and saturation, with its own reset. These eight controls affect only the exhaust. Muffler geometry controls require a fitted muffler.
5. Use direct RPM/load, the comparison cycle or simulated driving. Space toggles playback. Listening volume spans 0–1. Wheel adjustments use discrete steps; Shift makes finer adjustments.
6. Save the project or render a WAV. Old projects remain readable and use the physical engine for B; removed sound controls no longer select another rendering path.

**Intake air noise** adjusts airflow hiss independently of pipe pulsations and turbo whine. Its new default is about -42 dB relative to the old airflow setting; 0 dB restores that setting and Off silences the airflow component. This changes source balance, while the intake level still controls the whole layer. Projects missing the new control receive its default. Regenerate WAVs and BeamNG exports to apply changed sound settings.

Imports use the matching Automation Variant bore/stroke, compression, cam setting, redline and supported component tags when present. Both imported engines and free creations expose the same complete builder, detailed tuning and physical dyno. The parameter-origin table distinguishes imported, calculated, estimated and modified values. Restore any section to the imported reference without losing edits in the other sections. Firing order, detailed valve timing, acoustic dimensions and other unavailable properties remain explicit estimates; this is a reconstruction, not an exact manufacturer engine model.

Intake and exhaust cams have independent duration, lift and advance, with separate resets. Older projects retain their shared cam baseline. The intake section distinguishes total equivalent throttle area from the diameter of each individual body.

BESS prefers a compatible 48 kHz Windows output. Exported WAVs are mono 48 kHz / 24-bit PCM. **Reconnect audio** applies a device change. Imports and exports run in the background. A 3 ms lookahead limiter protects listening peaks; a muffler changes acoustics and does not guarantee that the mixed signal stays below full scale.

Intake sound combines damped valve-flow pulses with airflow-driven breath. Mechanical sound uses short textured contacts at valve/injection events, shaped by the pitch and resonance controls. Both belong to the same physical engine; their level sliders control the corresponding layers.

## BeamNG export

Use **BeamNG export** at the top of the listening panel to reach the export section. Enter a **Sound profile name**, then select **Create BeamNG configuration…**; file-level analysis is optional. The section shows the exhaust decay and back-pressure settings used for rendering. Place the generated add-on ZIP in BeamNG’s active mods folder. Keep the original Automation vehicle enabled beside the new add-on ZIP. Choose its **(BESS - Profile)** configuration in BeamNG. Different profile names produce separate paths and can coexist.

The add-on contains physical exhaust and engine-side loops, sound blends and configuration metadata. It preserves the original vehicle physics and its game startup, turbo and afterfire references. Listening volume and live A/B level matching do not change exported file levels. Per-knot gain calibration uses the source levels, with bounded gain and peak headroom; it does not feed the source waveform into the physical engine.

The saved complete engine definition drives both loop rendering and level analysis. Export retains the source RPM/load sample grid. Its steady off-load loops disable fuel cut and the starter: they do not encode a triggered BESS startup, turbo spool history or lift-off afterfire. Those events continue to use the vehicle's existing game behavior and references. Exhaust decay and back pressure are baked into the rendered loops; they do not add in-game sliders or change BeamNG vehicle torque. Export level calibration can compensate changes to **Coupled exhaust level**, which is a volume correction rather than feedback strength. Regenerate the add-on after changing the sound. Changing the workshop's redline does not change vehicle physics or extend the exported sample grid.

The **BeamNG two-emitter preview** approximates the engine/exhaust balance and camera position in mono. Its cabin and distance filters are listening perspectives. Actual BeamNG spatial mixing, vehicle gains and loop interpolation still require in-game testing. Scratch-only projects export WAVs; BeamNG export requires an imported vehicle.

## Projects and driving

Version 4 projects retain the complete edited physical engine, its imported baseline, fingerprints of source audio and verified engine data, layer levels, driving settings and profile name. Versions 1–3 remain readable and migrate on saving. Keep the source ZIP in place: changed audio or engine data must be reimported explicitly. Older projects acquire their engine-data reference on opening and retain it when saved. No source vehicle is overwritten by loading or exporting.

Simulated driving uses the physical crank and engine torque for both origins, with throttle, brake, gears, wheel resistance, accessories and a hold-to-start command. A unique imported vehicle configuration supplies declared mass, up to twelve forward ratios, final drive, tyre radius and clutch reference when its active parts provide unambiguous values. Missing data retain adjustable estimates and are labelled. Reopening an existing project retains its saved driving setup; **Restore available vehicle values** applies source values deliberately. Direct mode holds the selected operating point. Comparison mode runs idle, acceleration, lift-off and recovery. Imported direct controls and exported loops retain the source-bank RPM range; physical driving follows the edited engine's idle/redline, up to 12,000 RPM. Listening rooms affect audition only; WAV and BeamNG exports stay dry.

**Export selected mode** renders the current driving setup; it does not record earlier interactions. **Export A/B comparison** creates two 16-second clips matched by RMS for listening. Equal RMS does not mean equal perceived loudness.

## Advanced comparisons

**Experimental back pressure**, in **Exhaust sound**, enables exhaust feedback into gas flow and torque. Its response to primary length remains under qualification. **Measure coupled level for this engine** measures a fixed correction across operating points and displays the remaining level differences; it does not normalize the live sound. **Efficient listening rate** keeps high-rate gas substeps while running acoustics at the output rate. Both options are saved with the common engine and default off for existing projects.

**Finite-volume primaries — offline WAV quality** uses a more expensive conservative gas-wave calculation in the exhaust primaries. The 4 mm grid preserves small-signal accuracy through 4 kHz in the checked cases; I4 examples cost about 15–18 seconds per second of audio on the tested Ryzen 7 3800X. It is available for WAV and BeamNG rendering, and cannot be combined with experimental back pressure. Live listening stops while it is enabled; disable it to resume real-time audio.

**Calibrate a WAV against a reference…** fits a bounded linear filter from two stationary recordings at matching RPM, load and microphone conditions. It creates a new folder with a calibrated WAV, filter coefficients and separate held-out spectral measurements. This does not identify the engine's physical constants; neither input file is modified.

## Development

```powershell
cargo run --release --bin bess -- --open vehicle.zip
cargo run --release --bin bess -- --compare vehicle.zip output/comparison
cargo run --release --bin bess -- --beamng vehicle.zip output/beamng-profile
cargo run --release --bin bess -- --drive-demo vehicle.zip output/driving
cargo run --release --bin bess -- --render output/scratch.wav
cargo run --release --bin bess -- --audio-check output/audio-check.txt vehicle.zip 30
cargo run --release --bin bess -- --audio-check output/v12-hall.txt scratch 600 "V12 60°" cycle hall native,coupled
cargo run --release --bin bess -- --calibrate-wav generated.wav reference.wav output/new-calibration
cargo test --release --all-targets
cargo clippy --release --all-targets -- -D warnings
cargo fmt --check
graft build
graft check
```

BDSP is pinned in `Cargo.toml` and `Cargo.lock` to `b2981d431d321d29224752a2f855495da965d4bb`. Building requires Rust/MSVC and access to that private Git dependency. The compiled Windows application runs without repository access. Vehicles, sample banks and local listening artifacts are not bundled in the repository.

See the [operational index](docs/INDEX.md) for current work and evidence. Dated reports document earlier snapshots; the current product has only the original A reference and physical B engine. Signal tests and successful exports do not establish subjective realism or in-game acceptance.
