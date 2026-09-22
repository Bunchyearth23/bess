# BESS 0.8.4

BESS 0.8.4 improves the Cerberus sound around 5,200 RPM. Prepared source loops now play continuously, and neighboring RPM samples share a consistent cycle duration. This reduces the repeated level jumps heard at steady RPM.

The application, generated pages, export instructions, and public documentation are in English. Live playback prefers 48 kHz when supported by the output device. Exported WAV files are mono 48 kHz / 24-bit PCM.

## Portable Windows package

Download `BESS-0.8.4-Windows-Portable.zip`, extract the entire folder, and open `START.html`. It includes `BESS.exe`, twelve Automation source archives, A/B/C listening comparisons, six focused Cerberus clips, and twelve BeamNG vehicle copies.

The source ZIPs and BeamNG copies are release assets rather than files in the Git checkout. Keep the extracted folder together so the included projects can resolve their relative source paths.

The BDSP source dependency is currently private. Building BESS from this public Git checkout requires access to BDSP; the packaged Windows executable runs without it.

## Verification

The 47 Rust tests pass. All twelve source A renders remain byte-identical to their 0.8.3 references; all twelve B renders were recalculated. At a held 5,200 RPM, the 50 ms RMS variation coefficient fell from 0.121 to 0.078 at full load and from 0.218 to 0.112 at light load. Twelve BeamNG copies were reimported; 688 sound loops were replaced while 1,194 other ZIP entries were preserved.

These checks establish file integrity and measured signal behavior. Naturalness still requires listening, and BeamNG driving behavior still requires an in-game test.
