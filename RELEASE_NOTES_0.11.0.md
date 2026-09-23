# BESS 0.11.0

Version 0.11.0 improves both B listening modes after user feedback. Standard source-guided resynthesis reduces a regular low-RPM exhaust drone, balances unusually loud recorded idle points, and softens high-load Intake crackle. The experimental generated mode gives throttle/load rises a stronger pressure attack and reduces the metallic airflow and electrical-like background noise. Automation A remains the source reference. The experimental mode is still a live listening option; BeamNG export, preview and level estimates use standard source-guided B.

Standard B now has an explicit Automation timbre blend: 100% keeps recorded excitation inside BESS processing, while 0% uses descriptors measured from the ZIP without replaying its PCM in B. Sound profiles can be named and exported as separate selectable BeamNG configurations for the same source vehicle. Six sound characters and the simpler/advanced control sections remain available.

A separate recording-free four-stroke engine instrument is now included as a development tool. It has versioned JSON configurations, three generic presets, real-time terminal controls, reproducible mono 48 kHz / PCM24 WAV scenarios and a render benchmark. It does not require Automation audio and does not replace the main BESS GUI or its BeamNG export.

## Windows downloads

- `BESS-0.11.0-Windows-Portable.zip`: main application, start page, guide, release notes and file hashes.
- `BESS-0.11.0-Windows.exe`: main application without the portable folder.
- `BESS-0.11.0-Standalone-Windows-Portable.zip`: recording-free engine and live terminal executables, three generic presets and quick-start guide.
- Each archive and executable has a matching `.sha256` sidecar.

The packages contain no test vehicles, Automation sound banks, prepared BeamNG add-ons or listening corpus. Import your own Automation vehicle ZIP into the main application. Keep the original vehicle ZIP beside a new BESS configuration add-on and remove an older add-on for the same vehicle before enabling it.

The source checkout requires the pinned private BDSP dependency to build, while the Windows binaries run without repository access. Automated tests, spectral checks and package hashes do not establish how a vehicle sounds inside BeamNG; in-game listening remains the final check.
