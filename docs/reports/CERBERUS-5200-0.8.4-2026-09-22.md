# BESS 0.8.4 — Cerberus level near 5,200 rpm

## Report and cause

At a steady 5,200 rpm, between Automation's 4,989 and 5,338 rpm samples, BESS 0.8.3's B render varied in level much more than A playback. The defect was reproducible in rendered files independently of the audio device. A pseudo-random segment change occurred every two 720-degree cycles, about every 46 ms at this rpm. Crossfading prevented isolated clicks, but adjacent segments did not always retain the same amplitude and local phase.

A second drift came from estimating cycle length independently for each sample. When mixing neighboring recordings, their phases evolved at different rates. Each recording alone was stable; fluctuation arose mainly between the two rpm points. [Jagla et al. (2012)](https://pubmed.ncbi.nlm.nih.gov/23145595/) and [Chen and Zhang (2021)](https://www.extrica.com/article/21920/pdf) discuss cycle-synchronized joins and phase/amplitude tradeoffs. The specific BESS cause is established by the measurements below and reader inspection, not simply attributed to those papers.

## Correction

B now follows the recorded loop prepared at import in temporal order, without new anchor jumps every two periods. The cycle duration implied by each sample's rpm forms a common basis for interpolating recordings. Pressure front, texture, controlled cycle variation and other B controls remain available. A playback and original Automation ZIPs are unchanged.

## Reproducible measurements

The numeric check compares eight seconds held at 5,200 rpm, 35% volume, 50 ms RMS windows and the first second excluded. The coefficient of variation is RMS-level standard deviation divided by its mean; lower values indicate steadier level in this test.

| Load | A source | B 0.8.3 | B 0.8.4 |
| --- | ---: | ---: | ---: |
| Full | 0.064 | 0.121 | **0.078** |
| Low | 0.109 | 0.218 | **0.112** |

Both fixed A clips are byte-identical before and after. All twelve corpus comparison A WAVs are also byte-identical to 0.8.3, while all twelve B WAVs changed. Tracks remain mono **48 kHz / PCM24**. The constant-load Cerberus regression and all **47 release tests** pass; strict linting reports no warnings.

BESS regenerated and reimported twelve BeamNG copies. They replace 688 WAV loops, while 1,194 other entries and all twelve source ZIPs remain unchanged. The largest loop seam difference is 4.57 times the RMS of adjacent differences, below the technical limit of 8. The English release audit is `output/beamng-0.8.4-en/verification.json` in the project and `beamng/verification.json` in the delivery.

On the Sound BlasterX G6, a [30-second silent audio test](cerberus-0.8.4-english-audio-check.txt) of the final English build opened a 48 kHz stereo float stream: 2,999 callbacks, no 10 ms budget overruns and 1.522 ms maximum computation. This does not judge audible sound quality.

The English public delivery passed 189 SHA256 checks, 93 links, 36 portable projects, 36 comparison tracks, six fixed Cerberus tracks and all 688 BeamNG loops at 48 kHz / 24 bit. Its executable reimported Cerberus from the delivered sources and reproduced both packaged A/B corpus WAVs exactly. Formatting, 47 release tests and strict linting passed for the English build; the Graft graph passed before localization.

## Listening limits

BESS A playback linearly blends two samples and itself has an average dip of roughly 1.8–2 dB between 4,989 and 5,338 rpm. The correction addresses B's additional time-domain pumping, not that level curve inherited from blending. A is not a recording of Automation's or BeamNG's sound engine. These measurements do not establish perceived naturalness or defect-free BeamNG driving. The [final test guide](../FINAL-TESTS.md) calls for a steady 5,200 rpm listen and a slow sweep around it.
