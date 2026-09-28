# BESS — Bunchy's Engine Synthesis System

BESS is a Windows engine sound workshop built with Rust, egui and BDSP. It reads an Automation vehicle ZIP, builds a physical engine from its verified engine data, and exports a selectable sound configuration for BeamNG.drive. The same physical engine also powers engines designed from scratch.

There are two listening choices:

- **A · Source Automation** plays the original imported WAV bank across RPM and load. It receives no B sound shaping. Playback interpolation, listening volume and output safety still apply; this is not a recording of the Automation game.
- **B · BESS physical engine** generates cylinder pressure, valve flow, intake, exhaust and mechanical sound. The original waveform never enters B. The same engine supplies live listening, WAV renders and BeamNG exports.

## Use the application

1. Launch `BESS.exe` and choose **Import Automation ZIP…**, or **New engine from scratch**.
2. For an import, check the engine identity and the visible model assumptions. Cylinder layout and capacity must be verified against the selected sound bank and active vehicle engine. Missing required data leave A available and make B unavailable with an explanation.
3. Compare A and B. Set exhaust, intake and mechanical levels, engine layer gain and idle gain. Optional level matching helps compare tone in the live mix.
4. Shape the physical sound with 27 saved controls: shared tone and response, intake resonance/length, mechanical pitch/resonance, combustion and exhaust geometry. **Exhaust tone** adds separate bass, body gain/frequency/width, rasp, low/high cuts and saturation, with its own reset. These eight controls affect only the exhaust. Muffler geometry controls require a fitted muffler.
5. Use direct RPM/load, the comparison cycle or simulated driving. Space toggles playback. Listening volume spans 0–1. Wheel adjustments use discrete steps; Shift makes finer adjustments.
6. Save the project or render a WAV. Old projects remain readable and use the physical engine for B; removed sound controls no longer select another rendering path.

Imports use the matching Automation Variant bore/stroke, compression, cam setting, redline and supported component tags when present. Firing order, detailed valve timing, acoustic dimensions and other unavailable properties are explicitly estimated. This is a reconstruction, not an exact manufacturer engine model. Scratch projects expose the engine builder directly.

BESS prefers a compatible 48 kHz Windows output. Exported WAVs are mono 48 kHz / 24-bit PCM. **Reconnect audio** applies a device change. Imports and exports run in the background. A 3 ms lookahead limiter protects listening peaks; a muffler changes acoustics and does not guarantee that the mixed signal stays below full scale.

Intake sound combines damped valve-flow pulses with airflow-driven breath. Mechanical sound uses short textured contacts at valve/injection events, shaped by the pitch and resonance controls. Both belong to the same physical engine; their level sliders control the corresponding layers.

## BeamNG export

Enter a **Sound profile name**, optionally calculate the exported file levels, then select **Create BeamNG configuration…**. Keep the original Automation vehicle enabled beside the new add-on ZIP. Choose its **(BESS - Profile)** configuration in BeamNG. Different profile names produce separate paths and can coexist.

The add-on contains physical exhaust and engine-side loops, sound blends and configuration metadata. It preserves the original vehicle physics and its game startup, turbo and afterfire references. Listening volume and live A/B level matching do not change exported file levels. Per-knot gain calibration uses the source levels, with bounded gain and peak headroom; it does not feed the source waveform into the physical engine.

The **BeamNG two-emitter preview** approximates the engine/exhaust balance and camera position in mono. Its cabin and distance filters are listening perspectives. Actual BeamNG spatial mixing, vehicle gains and loop interpolation still require in-game testing. Scratch-only projects export WAVs; BeamNG export requires an imported vehicle.

## Projects and driving

Version 3 projects retain fingerprints of the source audio and verified engine data, physical sound controls, layer levels, driving settings and profile name. Versions 1 and 2 remain readable. Keep the source ZIP in place: changed audio or engine data must be reimported explicitly. Older projects acquire their engine-data reference on opening and retain it when saved. No source vehicle is overwritten by loading or exporting.

Simulated driving provides RPM/load from throttle, brake, gears, wheel resistance and grade. Its vehicle mass, gears and torque settings are bench estimates rather than an identification of the imported vehicle. Direct mode holds the selected operating point. Comparison mode runs idle, acceleration, lift-off and recovery. The source-bank RPM range bounds imported playback; the physical solver supports up to 12,000 RPM.

**Export selected mode** renders the current driving setup; it does not record earlier interactions. **Export A/B comparison** creates two 16-second clips matched by RMS for listening. Equal RMS does not mean equal perceived loudness.

## Development

```powershell
cargo run --release --bin bess -- --open vehicle.zip
cargo run --release --bin bess -- --compare vehicle.zip output/comparison
cargo run --release --bin bess -- --beamng vehicle.zip output/beamng-profile
cargo run --release --bin bess -- --drive-demo vehicle.zip output/driving
cargo run --release --bin bess -- --render output/scratch.wav
cargo run --release --bin bess -- --audio-check output/audio-check.txt vehicle.zip 30
cargo test --release --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --check
graft build
graft check
```

BDSP is pinned in `Cargo.toml` and `Cargo.lock` to `b2981d431d321d29224752a2f855495da965d4bb`. Building requires Rust/MSVC and access to that private Git dependency. The compiled Windows application runs without repository access. Vehicles, sample banks and local listening artifacts are not bundled in the repository.

See the [operational index](docs/INDEX.md) for current work and evidence. Dated reports document earlier snapshots; the current product has only the original A reference and physical B engine. Signal tests and successful exports do not establish subjective realism or in-game acceptance.
