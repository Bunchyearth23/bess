# Requested rpm — 0.3.1 fix

The slider and numeric input use `Bank.min_rpm..=Bank.max_rpm`. Initial rpm and older-project values are clamped to that range. Synthesis also enforces the bounds in construction and commands, so automatic cycles and WAV rendering do not request transposition outside the bank. Global validation accepts the blend metadata envelope (200–20,000 rpm), while vehicle-specific limits constrain actual use.

All 19 existing tests, strict linting and formatting passed. Executable: `dist/BESS-0.3.1.exe`. SHA256: `63CFE1E31FE29E63E0C3332010D8715497B7E705D6A3B9D68A42109F3359B39A`. A post-import window capture was inspected historically and is excluded from the public package because it showed the former French UI.
