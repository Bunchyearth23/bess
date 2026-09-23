# Intake, mechanics and standalone noise correction — 2026-09-23

## User report

The user heard metallic hiss and electrical buzzing in every initial standalone WAV. In the BESS window the fault was confined to B, especially Intake and Mechanics, in both standard and experimental B. This correction remains subject to the user's listening.

## Changes

- The standalone default previously added sample-wise residual noise after the exhaust ducts and sustained a 180 Hz block resonance. Residual roughness is now filtered and carried by event excitation before the duct. The default residual is zero; the block is 125 Hz, 12 ms, level 0.025. These remain adjustable calibration controls.
- The generated B voice had an ungated mechanical noise band and bright intake/mechanical bands. Mechanical noise now follows events only, upper bands are reduced, and Intake and Mechanics have separate warm low-pass filters.
- Standard B also emphasized high-frequency recorded residual and mixed a high-pass mechanical path with periodic impacts. Intake and Mechanics each have a low-pass filter; the engine-air band and impact level were reduced and warmed.

## Checks

The old running `target/release/bess.exe` rendered a Cerberus baseline at 3,000 rpm and 0.65 load. The corrected code rendered the same three matched-level tracks with `examples/noise_review.rs`. The source A WAV SHA-256 is identical before and after. Four B control-difference clips in `output/b-noise-fix-cerberus/` isolate Intake or Mechanics at full setting. Each isolated clip is adjusted to 0.035 RMS for fault-finding; this gain is not its normal BESS mix level.

`tools/measure_noise_bands.py` measures settled PCM24 RMS above a one-pole 4 kHz crossover. At equal 0.035 RMS, between the first and final local correction pass, the isolated component changed as follows:

| B control | Earlier high-pass RMS | Corrected high-pass RMS | Change |
| --- | ---: | ---: | ---: |
| Standard Intake | 0.01130 | 0.00898 | −2.0 dB |
| Standard Mechanics | 0.01758 | 0.00645 | −8.7 dB |
| Experimental Intake | 0.01089 | 0.00546 | −6.0 dB |
| Experimental Mechanics | 0.01080 | 0.00450 | −7.6 dB |

The final B comparison also exists at 1,600 rpm and 0.20 load in `output/b-noise-fix-cerberus-low/`. The full B mix changes only slightly because Intake and Mechanics are normally quieter than exhaust. The isolated tracks expose the affected components for listening.

Six corrected standalone WAVs are in `output/standalone-noise-fix-20260923/`; the original six remain in `output/standalone-validation-20260923/`. Four-even steady still has 400 combustion events in 4 seconds at 3,000 rpm and now has 0.028167 AC RMS and 0.070276 peak; the previous RMS was 0.029461. The single-cylinder clip's strongest resolved peak moved from the former 175.78 Hz block resonance to about 123.05 Hz. No ±0.98 ceiling hit was found in the corrected steady clips. The JSON presets were regenerated from the corrected defaults.

The full release test suite and strict Clippy passed after the final changes. The corrected GUI was initially built separately at `target/noise-fix/release/bess.exe` while the previous executable was locked. After the user closed that process, the usual `target/release/bess.exe` and both standalone binaries were rebuilt. Its command-line comparison reproduced all three corrected Cerberus WAVs byte for byte. Automated and spectral checks do not establish subjective realism or prove that the new GUI has been heard on the user's device.
