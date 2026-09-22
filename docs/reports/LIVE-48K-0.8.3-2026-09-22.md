# BESS 0.8.3 — 48 kHz playback, 24-bit exports retained

## Change

Version 0.8.2 requested the Windows default output format. On the Sound BlasterX G6, that was 192 kHz, so BESS computed 192,000 audio frames per second. Version 0.8.3 first selects a compatible 48 kHz stream. If no usable 48 kHz stream is advertised, it keeps the device default. The actually opened format appears in the application.

Internal synthesis remains floating point. On this machine the G6 accepts 48 kHz stereo 32-bit float, with the same mono sample sent to both channels. The device's 32-bit float describes live transport, not file precision. All new comparison WAVs and replacement WAVs in BeamNG copies remain **mono 48 kHz / PCM24**, as requested. Original Automation ZIPs are not modified.

## Evidence

- [Final G6 test](live-48k-0.8.3-audio-check.txt): 30 seconds of silent synthesis, 3,000 callbacks, no 10 ms budget overruns and 2.010 ms maximum. This measures computation and stream opening, not perceived quality at audible volume.
- All twelve imports under `output/corpus-0.8.3-pcm24` succeeded. The 24 A/B WAVs at 48 kHz / 24 bit have the same SHA256 as their 0.8.2 counterparts: changing the playback rate did not change rendered synthesis or quantization.
- All twelve copies under `output/beamng-0.8.3-pcm24` were reimported by BESS. They replace 688 WAV loops at 48 kHz / 24 bit; 1,194 other entries remain identical. The largest seam-difference-to-neighbor-difference-RMS ratio is 4.36, below the technical threshold of 8.
- 46 release tests and strict linting pass. A focused test covers 48 kHz selection and device-default fallback.
- The 0.8.3 complete delivery passed 194 SHA256 checks, 78 local links, 36 portable projects and 36 comparison tracks. Its executable reimported Genesis from the delivered sources and reproduced the two expected A/B WAVs exactly.

## Limits

48 kHz selection depends on formats reported by the chosen output. BESS shows the fallback format when another device does not advertise 48 kHz. A silent 30-second test cannot guarantee uninterrupted playback over a long audible session. Vehicle copies passed archive checks and reimport, but have not been driven in BeamNG.

Interrupted 16-bit experiments remain in local `output/corpus-0.8.3` and `output/beamng-0.8.3` folders. Automatic command review rejected their recursive deletion. They are excluded from delivery; only `-pcm24` folders were used to assemble BESS 0.8.3.
