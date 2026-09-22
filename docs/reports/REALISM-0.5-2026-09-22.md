# BESS 0.5 realism demo — corpus and first components

## Delivery

- Executable: `dist/BESS-0.5.0.exe`, SHA256 `28716E9C9CF8F1B5EB6F33BEA0FF2969AF496EDC150F1DF09D525F858EC7F87A`.
- Demo: `output/corpus-0.5.0/index.html`, directly openable in a browser. This session's local server was `http://127.0.0.1:8768/`; the file continued to work when the server stopped.
- Previous reference retained: `output/corpus-0.4.1/`.
- Reports and provenance: `corpus.json` in each folder; a 0.5 report copy was placed under `docs/reports/corpus-0.5.json`.
- A native capture of the 0.5 UI was inspected historically. It is excluded from the public package because the former interface was in French.

This was the plan's first delivery, not completion of all seven steps.

## Real corpus

BESS's Rust reader imported twelve ZIPs without extracting or modifying the archives. Each had a supported blend, two load layers and a distinct bank according to a hash over layer, rpm and WAV bytes that ignores filenames. Binary distinctness does not prove twelve fully different acoustic signatures.

| Vehicle | RPM points per layer | Source range (rpm) | Accepted periods / WAVs |
| --- | ---: | ---: | ---: |
| Advent TC | 33 | 803–6,997 | 64 / 66 |
| Archo Coupe | 28 | 803–4,989 | 54 / 56 |
| B5 A | 26 | 803–4,358 | 49 / 52 |
| B5 C | 30 | 803–5,712 | 52 / 60 |
| B5 GT | 28 | 803–4,989 | 52 / 56 |
| Berlingo 1000lingo | 33 | 803–6,997 | 64 / 66 |
| Cerberus A | 33 | 803–6,997 | 64 / 66 |
| Genesis Phantom | 34 | 803–7,487 | 68 / 68 |
| Nord Optis | 24 | 803–3,806 | 47 / 48 |
| Thunderhawk Zero | 28 | 803–4,989 | 54 / 56 |
| Volk Icarus | 24 | 803–3,806 | 47 / 48 |
| Volk Icarus II | 23 | 803–3,557 | 46 / 46 |

Total: 688 WAVs, 661 accepted periods and 27 nominal fallbacks. The then-current inspector found no reliable cylinder count for any of the twelve; that did not establish absence from every possible ZIP metadata field. Later 0.8.2 `.car` parsing recovered declared counts with provenance checks.

The exploratory Python periodicity analysis used all-point FFT normalized correlation and found strong periodicity with small median adjustments. The Rust engine sampled bounded positions and additionally required agreement between window halves, so its decisions deliberately differed from the exploratory diagnostic.

## Synthesis changes

1. Local search ±3% around the 720-degree period implied by rpm, parabolic peak interpolation, 0.65 correlation threshold and 0.15% agreement between half-windows. Otherwise retain the nominal period. Analysis runs outside the callback and identifies neither combustion count nor firing order.
2. B loops prepared with the accepted period. A reference loops and gain separately retained as in 0.4.1. All twelve exported A WAVs were byte-identical to earlier references.
3. Cycle-synchronous mean at near-source density and complementary residual by subtraction. An uncertain period puts the entire signal into the residual. Components are not physically isolated intake, mechanical or combustion tracks.
4. Two adjustable, saved and smoothed gains: pulses and natural texture. Source-WAV residual replaces continuous added white noise for intake and mechanical layers. Lift-off events and optional turbo keep their own models.

The analysis is informed by [Julius O. Smith on autocorrelation and noise analysis](https://www.dsprelated.com/freebooks/sasp/Spectrum_Analysis_Noise.html). Thresholds, windows, fallback and separation are BESS choices, not a validated implementation of published PSOLA.

## Demo procedure

1. On the page, play a vehicle and switch A/B/C. A = replayed source, B = 0.5, C = 0.4.1; the same 16-second scenario and comparable RMS. RMS is not LUFS. A brief interruption may occur when another file loads.
2. For B5 A, Genesis Phantom, Thunderhawk Zero and Volk Icarus II, open the isolated-component clips. Compare original playback, measured cycles, pulses and texture. They follow a different scenario: slow sweep, endpoint holds, then fast sweeps and load changes. There is no acoustic enhancement; components share one gain, with no separate texture normalization.
3. In BESS 0.5, open the vehicle's B project. Choose direct or comparison-cycle driving, listen and change the two gains. At 1/1, components recombine before acoustics; 0/1 and 1/0 expose their contributions. Diagnostic page clips are better for hearing components without effects.
4. Save, reopen and export the selected mode. Tests cover parameters and common playback/render transport; the user's interactive subjective assessment was still outstanding.

## Evidence obtained

- 12/12 imports, 24 16-second renders in mono 48 kHz PCM24 and 24 projects saved/reloaded for each reference version. No import error, silent or saturated output. A/B RMS difference <0.001 dB for every pair.
- Twelve previous-version C references and 16 isolated clips added. B projects retained settings from before final comparison-WAV level matching.
- Maximum absolute reconstruction error over the four vehicles was ≤2.9802322e-8 before PCM conversion. This proves numeric complementarity, not perfect acoustic separation.
- 30 release tests covered known fractional period, silence/noise/drift rejection, reduced drift on known synthetic signal, reconstruction with nonperiodic residual, new gain effects, existing controls, zero allocation and playback/render consistency. Formatting and strict linting passed.
- A native capture showed Genesis imported, new gains and separate driving. Browser playback and B/C switching were observed with 16-second duration and preserved progress; this was not an auditory judgment. The in-app browser crashed on the first 0.4.1 reference open, then the page reloaded and played.
- Real CPAL test on Genesis: 30 seconds on Sound BlasterX G6 at 192 kHz stereo, 2,999 callbacks, 5.524 ms maximum and no overrun. **Output volume was zero**, so it did not prove absence of audible artifacts. The historical raw report was `realism-0.5-audio.txt`.

## Plan state at that point

- Step 1: corpus demo delivered and operation verified, without subjective superiority validation.
- Step 2 partial: periods measured and fallback and playback demo available. Synchronized segment playback and improved neighbor transitions remained; long loops and profile alignment were still used.
- Step 3 partial: approximate separation, gains and component clips delivered. Comparative listening and modulation validation without identity loss remained.
- Steps 4–7 still open then: rpm/load maps, acoustic calibration, documented engine-event excitation, then BeamNG export and demonstration.

A/B preparation retained two loop copies, increasing memory use. The ceiling is 24 million decoded samples, not 24 MB total memory. B renders of older projects change; earlier demos and the 0.4.1 executable were retained.

## Reproduction

From the repository root, use a new folder to avoid overwriting a reference:

```text
cargo run --release --example corpus -- cars output/new-reference
cargo run --release --example components -- cars/bunchyearth23_genesis_phantom.zip output/new-reference/vehicle-08/components
python tools/corpus_page.py output/new-reference output/corpus-0.4.1
```

HTML generation and Rust comparisons do not require NumPy; `tools/analyze_corpus.py` uses it for exploratory diagnostics. Example source ZIPs are included in the complete release asset rather than the source checkout.
