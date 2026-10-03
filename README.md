# BESS — Bunchy's Engine Synthesis System

BESS is a Windows engine sound workshop built with Rust, egui and BDSP. It reads an Automation vehicle ZIP, builds a physical engine from its verified engine data, and exports a complete vehicle ZIP with the original configuration and a separate selectable BESS sound variant for BeamNG.drive. The same physical engine also powers engines designed from scratch.

There are two listening choices:

- **A · Source Automation** plays the original imported WAV bank across RPM and load. It receives no B sound shaping. Playback interpolation, listening volume and output safety still apply; this is not a recording of the Automation game.
- **B · BESS physical engine** generates cylinder pressure, valve flow, intake, exhaust and mechanical sound. The original waveform never enters B. The same engine supplies live listening, WAV renders and BeamNG exports.

## Use the application

1. Launch `BESS.exe`. BESS finds the BeamNG user folder from its launcher settings and lists the Automation archives under **Source vehicle**. Select **Import** beside a vehicle, use **Import Automation ZIP…** for another file, or choose **New engine from scratch**. **Choose folder…** remembers a custom location; **Detect BeamNG** returns to automatic discovery.
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

Use **BeamNG export** at the top of the listening panel, name the variant, then export the complete vehicle and BESS variant. The default destination is **BESS-exports** beside the detected `mods` directory. Each run creates a new output folder containing the complete ZIP, its project, manifest and instructions. You can save another default destination or choose a one-off export folder. The source archive is preserved. BESS leaves vehicle merging to BABM and does not install or reorganize game mods.

Export progress stays visible at the top of the window: preparation, sound generation, ZIP writing and verification. It shows completed sounds, RPM/load, elapsed time and an estimated remaining time for sound generation. Independent sounds are calculated in parallel without changing their quality settings. **Cancel export** stops the work safely; completion is reported only after the package has been verified.

The ZIP preserves every original vehicle file, configuration and sound. It adds a named BESS configuration with independent engine/intake and exhaust sound routes, cloned engine part, blend files and WAVs. Select the configuration labelled **BESS** in BeamNG to hear the new sound, or the original configuration to hear the unchanged Automation sound. The complete edited engine, including exhaust decay and back pressure, drives rendering. These settings are baked into the loops; they do not add game sliders or change BeamNG torque. Listening volume and live A/B level matching are not exported. The calibrated exhaust and bounded companion stem retain the selectable variant's two-emitter balance.

For a direct in-game test without BABM, enable only this complete ZIP for the vehicle: it already contains both the original and BESS configurations. Regenerate the ZIP after changing the sound. Original startup, turbo and afterfire references are retained; stationary loops do not encode a triggered BESS startup, spool history or lift-off afterfire. Scratch-only engines export WAVs.

The library reads the detected `mods` directory, its direct `repo` child and manifest-listed individual originals under BABM's `.babm_backup`. Original `.zip.merged_backup` archives are labelled **original before grouping** and can be read without renaming them. Duplicate originals are shown once; processed full BESS ZIPs and old add-ons are excluded from the automatic source list. Multi-vehicle/multi-blend archives are unavailable for individual import; choose their preserved original archives. Neither discovery nor import activates or modifies these sources. The scan is bounded and ignores invalid, ZIP64/multipart or oversized archives.

With **BABM**, choose **Edit sound in BESS** on an original, export its complete vehicle and variant ZIP in BESS, then use **Open BABM…** and import the BESS variant. Both selected folders are passed to BABM. Exports also appear through default-folder discovery, including BESS's saved output choice. BABM adds the variant to the matching standalone vehicle or existing grouped pack, preserving the original sound, other trims, variables and configurations. Re-exporting the same named variant updates its own files. BABM also handles merging the complete ZIP with other vehicles. See the [workflow and exchange contract](docs/BESS-BABM-EXCHANGE.md).

The two-emitter preview and level estimate describe the BESS variant's sound routing. The historical add-on-only (`--beamng`) and sound-replacement (`--beamng-replacement`) exporters remain available to explicit command-line callers; `--beamng-complete` produces the current complete vehicle with its selectable variant. Actual BeamNG sound still requires in-game testing.

**Engine sound gain** controls the whole generated BESS sound from **0** (silent)
to **1** (unchanged level). It is saved with the project and applies to listening,
WAV output and both BeamNG sound emitters after level calibration. It does not
change the original A recording or the original configuration. The listening and
comparison-WAV volume remains separate and does not set the BeamNG export level.

## Projects and driving

Version 4 projects retain the complete edited physical engine, its imported baseline, fingerprints of source audio and verified engine data, layer levels, driving settings and profile name. Versions 1–3 remain readable and migrate on saving. Keep the source ZIP in place: changed audio or engine data must be reimported explicitly. Older projects acquire their engine-data reference on opening and retain it when saved. No source vehicle is overwritten by loading or exporting.

Simulated driving uses the physical crank and engine torque for both origins, with throttle, brake, gears, wheel resistance, accessories and a hold-to-start command. A unique imported vehicle configuration supplies declared mass, up to twelve forward ratios, final drive, tyre radius and clutch reference when its active parts provide unambiguous values. Missing data retain adjustable estimates and are labelled. Reopening an existing project retains its saved driving setup; **Restore available vehicle values** applies source values deliberately. Direct mode holds the selected operating point. Comparison mode runs idle, acceleration, lift-off and recovery. Imported direct controls and exported loops retain the source-bank RPM range; physical driving follows the edited engine's idle/redline, up to 12,000 RPM. Listening rooms affect audition only; WAV and BeamNG exports stay dry.

**Export selected mode** renders the current driving setup; it does not record earlier interactions. **Export A/B comparison** creates two 16-second clips matched by RMS before the BESS engine sound gain is applied for listening. Equal RMS does not mean equal perceived loudness.

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
