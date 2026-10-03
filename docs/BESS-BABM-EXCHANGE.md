# BESS / BABM sound exchange, version 1

## User workflow

1. In BABM, choose **Edit sound in BESS** for an individual original. Grouped vehicles use their preserved original archives. BESS also lists those originals in its detected BeamNG library, without activating or moving them.
2. In BESS, adjust the engine, name the variant, and export the complete vehicle with its BESS variant. The ZIP contains every original file unchanged plus a separately selectable BESS configuration and its own sound files. Save the project to continue editing later.
3. Choose **Open BABM…**, or open BABM and refresh **BESS sounds**. BESS passes its selected mods folder and the parent of its last successful export when opening the companion. An export saved elsewhere is therefore discoverable without changing the saved default. BABM otherwise discovers the sibling `BESS-exports` directory and BESS's saved output preference. One level of per-export subdirectories is scanned.
4. Review the matched vehicle and select **Import BESS variant**. Discovery is read-only; importing is explicit. BABM adds the variant to the standalone vehicle or remaps its paths into an existing grouped pack. The original configuration and sounds remain intact.
5. Repeat after another edit. The same source and profile identify the same variant. BABM checks its previously imported files before replacing only that variant's owned files. Other profiles, trims, variables and original configurations remain intact. Applied, older, conflicting or unavailable exports have distinct states.

In BeamNG, choose the configuration labelled **BESS** to hear the generated sound; the original trim retains the Automation sound. BABM owns vehicle merging. A complete BESS ZIP already includes its original, so an initial merge must not duplicate that original when both source archives are present. Actual in-game audio and ergonomics remain user acceptance checks.

BABM preserves pristine sources and prior archive revisions. BESS can rediscover the pristine original after both standalone and grouped updates. Unmerging restores original individual source archives. Neither application silently installs a second full vehicle alongside its original.

When this source already received a legacy sound replacement, importing its first
selectable variant restores the original WAVs from the pristine backup in the same
atomic update. BABM first verifies that the installed WAVs still match its legacy
receipt. Missing originals or outside edits block the migration; unrelated sources
and older independent variants remain untouched.

## Current complete-variant contract

The complete ZIP carries `bess-export.json` with a distinct kind. It is not a WAV
replacement request: its added configuration, JBeam and blend routing must travel
with the added WAVs.

```json
{
  "version": 1,
  "kind": "bess-variant-vehicle",
  "source_archive_sha256": "64 lowercase hexadecimal characters",
  "source_archive_name": "original.zip",
  "vehicle_root": "vehicles/original_vehicle/",
  "blend_path": "art/sound/blends/original.sfxBlend2D.json",
  "exported_at_unix_ms": 1791057600000,
  "variant_id": "stable identity derived from source and profile",
  "configuration_path": "vehicles/original_vehicle/bess_configuration.pc",
  "added_files": [
    {"path": "vehicles/original_vehicle/bess_configuration.pc", "sha256": "SHA-256 of the added bytes"}
  ]
}
```

`added_files` lists every added member except the marker itself. Each path is
disjoint from the original archive. The source hash identifies the pristine
archive; it is not the hash of the larger complete export. BABM validates the
added payload, remaps only the added vehicle paths as needed, and records ownership
and transformed hashes. Initial merging also compares the original members against
the pristine source when that source is available among the inputs or backups.
Without that separate source, the added-file checks do not independently establish
the provenance of the original members. An external change or a collision blocks
replacement instead of overwriting unrelated content.

## Legacy full-replacement contract

The explicit `--beamng-replacement` command retains `bess-full-vehicle` below.
For these older exports, **Apply BESS sounds** replaces the original trim's sound
files and does not create a new configuration. Exhaust-only intermediate renders
and historical add-on-only ZIPs do not use this replacement contract.

```json
{
  "version": 1,
  "kind": "bess-full-vehicle",
  "source_archive_sha256": "64 lowercase hexadecimal characters",
  "source_archive_name": "original.zip",
  "vehicle_root": "vehicles/original_vehicle/",
  "blend_path": "art/sound/blends/engine.sfxBlend2D.json",
  "exported_at_unix_ms": 1791057600000,
  "sounds": [
    {
      "path": "art/sound/engine/sample.wav",
      "original_sha256": "SHA-256 of the pristine source WAV bytes",
      "rendered_sha256": "SHA-256 of the exported WAV bytes"
    }
  ]
}
```

The original archive hash binds an export to a concrete preserved source, independently of its filename or current location. Sound hashes cover complete WAV bytes. The selected vehicle root and blend identify the source layout. An inherited marker is accepted by BESS only when its version, vehicle, blend, complete sound set and current rendered hashes agree with the actual source archive. Every new export retains original hashes and gets a later export timestamp.

BABM validates the marker and actual payload, matches the preserved source, derives grouped vehicle-root remapping from its backup manifest, and verifies sound ownership. Shared ambiguous sound paths, incomplete backup sets, unsupported versions, malformed paths, unexpected edits and conflicting revisions are refused rather than guessed. ZIP/path/metadata limits apply. Links and Windows reparse points cannot redirect backup or update writes.

Applying creates and verifies a staged archive, records the previous archive in history, and replaces the target atomically. The namespaced `babm-bess-updates.json` receipt travels inside the target archive so that audio and applied revision stay together. A per-mods write lock serializes application, merge and unmerge. Preserved source manifests list direct backup filenames; BESS does not trust arbitrary original absolute paths from those manifests.

## Command-line verification

```powershell
BABM.exe bess-scan --path C:\Test\mods --exports C:\Test\BESS-exports --json
BABM.exe bess-inspect C:\Test\BESS-exports\run\vehicle.zip --path C:\Test\mods --json
BABM.exe bess-apply C:\Test\BESS-exports\run\vehicle.zip --path C:\Test\mods --json
BABM.exe gui --path C:\Test\mods --exports C:\Test\BESS-exports
BESS.exe --open C:\Test\original.zip --beamng-mods C:\Test\mods --bess-exports C:\Test\BESS-exports
```

Companion folder arguments apply to the current session. Explicit user folder
choices remain separate from these launch overrides; opening a companion does
not silently replace the saved default folders.

The BESS repository's `tools/verify_babm_exchange.py` runs both compiled executables on disposable fixture directories. It checks standalone and grouped application, unchanged non-audio content, original rediscovery, repeated/stale application, BESS reopening BABM's output, and restoration. Its fixture inputs come from `tests/beamng_full_export.rs` when `BESS_BABM_TEST_ARTIFACTS` names a new directory. Runtime logs and executable hashes belong in the delivery report; the protocol alone is not proof of a successful game test.
