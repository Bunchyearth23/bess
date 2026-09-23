# BESS recording-free instrument

This separate engine synthesizer does not need an Automation vehicle ZIP. Extract the entire archive before running it. It uses the default Windows audio device for live playback.

Open PowerShell in this folder and render a six-second acceleration:

```powershell
.\standalone_engine.exe --preset four-even --scenario ramp --seconds 6 --out ramp.wav
```

Listen live:

```powershell
.\standalone_live.exe --preset four-even
```

At the live prompt, type `rpm 3000`, `load 0.5`, `volume 0.6`, `exhaust 0.7`, `intake 0.3`, or `block 0.2`. Type `quit` to close it. The included JSON files under `presets/` are editable examples; pass one with `--config presets/four-even.json` to either executable. `standalone_engine.exe --help` lists the available scenarios and options.

The instrument is a four-stroke sound model with externally commanded RPM and load. It is separate from BESS's Automation-guided GUI and BeamNG export.
