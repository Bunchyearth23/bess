"""Verify selectable BESS variants with the compiled BABM CLI on disposable copies.

Generate fixtures with BESS_VARIANT_TEST_ARTIFACTS while running the complete
variant integration test. Expected inputs: original.zip, exports/first.zip,
exports/revised.zip, and optional exports/other-profile.zip. No installed mod is
used. --output must not exist; commands and the final receipt stay there even if
a check fails. These are archive/CLI checks, not an in-game listening test.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import time
import zipfile
from pathlib import Path


MARKER = "bess-export.json"
STATE = "babm-bess-variants.json"
BOOKKEEPING = {MARKER, STATE}


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def byte_digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def members(path: Path) -> dict[str, str]:
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        assert len(names) == len(set(names)), f"Duplicate ZIP member: {path}"
        assert len(names) == len({name.lower() for name in names}), f"Case collision: {path}"
        result = {}
        for entry in archive.infolist():
            with archive.open(entry) as stream:
                result[entry.filename] = hashlib.file_digest(stream, "sha256").hexdigest()
        return result


def archive_json(path: Path, name: str) -> dict:
    with zipfile.ZipFile(path) as archive:
        return json.loads(archive.read(name))


def marker(path: Path) -> dict:
    result = archive_json(path, MARKER)
    assert result["version"] == 1 and result["kind"] == "bess-variant-vehicle"
    return result


def mapped(name: str, metadata: dict, target_root: str | None) -> str:
    root = metadata["vehicle_root"]
    return target_root + name[len(root):] if target_root and name.startswith(root) else name


def owned(metadata: dict, target_root: str | None = None) -> set[str]:
    return {mapped(item["path"], metadata, target_root) for item in metadata["added_files"]}


def preserve(before: dict[str, str], after: dict[str, str], except_names=()) -> None:
    exceptions = set(except_names)
    for name, expected in before.items():
        if name not in exceptions:
            assert after.get(name) == expected, f"Unrelated member changed or removed: {name}"


def verify_routing(target: Path, metadata: dict, target_root: str | None) -> dict:
    """Follow the actual selectable PC -> engine -> both blends -> WAV members."""
    with zipfile.ZipFile(target) as archive:
        names = set(archive.namelist())
        config_path = mapped(metadata["configuration_path"], metadata, target_root)
        config = json.loads(archive.read(config_path))
        part = config["parts"]["Camso_Engine"]
        engines = [archive.read(name).decode("utf-8") for name in owned(metadata, target_root)
                   if name.endswith(".jbeam")]
        matches = [text for text in engines if re.search(r'"' + re.escape(part) + r'"\s*:', text)]
        assert len(matches) == 1, f"Selectable engine part does not resolve uniquely: {part}"
        engine = matches[0]
        assert '"soundConfig"' in engine and '"soundConfigExhaust"' in engine
        samples = set(re.findall(r'"sampleName"\s*:\s*"([^"\\]+)"', engine))
        assert len(samples) == 2, f"Expected independent engine/exhaust sample names: {samples}"
        wavs = set()
        for sample in samples:
            blend_path = f"art/sound/blends/{sample}.sfxBlend2D.json"
            assert blend_path in names and blend_path in owned(metadata, target_root)
            blend = json.loads(archive.read(blend_path))
            assert len(blend["samples"]) == 2, "Missing unloaded/loaded blend row"
            for row in blend["samples"]:
                assert row, "Empty sample row"
                for sound, rpm, *_ in row:
                    assert rpm > 0 and sound in names and sound in owned(metadata, target_root)
                    wavs.add(sound)
        assert wavs == {name for name in owned(metadata, target_root) if name.endswith(".wav")}
        if target_root and target_root != metadata["vehicle_root"]:
            for name in owned(metadata, target_root):
                if name.endswith((".pc", ".json", ".jbeam")):
                    assert metadata["vehicle_root"] not in archive.read(name).decode("utf-8"), name
        return {"configuration": config_path, "engine_part": part, "emitters": len(samples),
                "resolved_wavs": len(wavs)}


def verify_payload(target: Path, export: Path, target_root: str | None,
                   merged: bool = False) -> dict:
    metadata = marker(export)
    current = members(target)
    assert owned(metadata, target_root) <= current.keys()
    with zipfile.ZipFile(export) as archive:
        for item in metadata["added_files"]:
            name = mapped(item["path"], metadata, target_root)
            data = archive.read(item["path"])
            assert byte_digest(data) == item["sha256"], item["path"]
            if target_root and item["path"].endswith((".pc", ".json", ".jbeam")):
                data = data.decode("utf-8").replace(metadata["vehicle_root"], target_root).encode()
            # Initial grouping adds a display-name tag to JBeam information.
            # Its registry and the routing checks below verify those transformed
            # bytes; an import after grouping must match the exact payload.
            if not (merged and name.endswith(".jbeam")):
                assert current[name] == byte_digest(data), f"Wrong imported payload: {name}"
    state = archive_json(target, STATE)
    revision = state["variants"][metadata["source_archive_sha256"] + ":" + metadata["variant_id"]]
    assert revision["exported_at_unix_ms"] == metadata["exported_at_unix_ms"]
    assert set(revision["files"]) == owned(metadata, target_root)
    assert all(current[name] == expected for name, expected in revision["files"].items())
    return verify_routing(target, metadata, target_root)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--babm", type=Path, required=True)
    parser.add_argument("--fixtures", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--catalogue", type=Path, help="Optional compiled BESS catalogue example")
    parser.add_argument("--chassis", default="Test Vehicle")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir()  # Exclusive ownership; never overwrite a prior proof.
    fixtures = args.fixtures.resolve()
    source = fixtures / "original.zip"
    first = fixtures / "exports/first.zip"
    revised = fixtures / "exports/revised.zip"
    other = fixtures / "exports/other-profile.zip"
    inputs = [source, first, revised] + ([other] if other.is_file() else [])
    source_hashes = {str(path): digest(path) for path in inputs}
    source_hash = source_hashes[str(source)]
    commands = []
    report = {"babm_sha256": digest(args.babm), "fixtures_sha256": source_hashes,
              "catalogue_checked": args.catalogue is not None, "scenarios": [], "passed": False,
              "boundary": "Compiled CLI and archive verification only; no in-game or listening check"}
    if args.catalogue:
        report["catalogue_sha256"] = digest(args.catalogue)

    def save_report() -> None:
        (output / "verification.json").write_text(json.dumps(report, indent=2), encoding="utf-8")

    def run(executable: Path, *arguments: object, as_json: bool = False,
            failure: bool = False):
        command = [str(executable.resolve()), *(str(arg) for arg in arguments)]
        started = time.monotonic()
        entry = {"command": command}
        try:
            result = subprocess.run(command, capture_output=True, text=True, encoding="utf-8-sig",
                                    errors="replace", timeout=300)
            entry.update(exit_code=result.returncode, stdout=result.stdout, stderr=result.stderr)
        except Exception as error:
            entry["error"] = str(error)
            raise
        finally:
            entry["elapsed_seconds"] = round(time.monotonic() - started, 3)
            commands.append(entry)
            (output / "commands.json").write_text(json.dumps(commands, indent=2), encoding="utf-8")
        if failure:
            assert result.returncode != 0, f"Expected refused import: {command}"
        elif result.returncode:
            raise RuntimeError(f"Command failed: {command}\n{result.stderr}\n{result.stdout}")
        return json.loads(result.stdout) if as_json and not failure else result

    def inspect(mods: Path, export: Path, status: str) -> dict:
        before = {str(path.relative_to(mods)): digest(path) for path in mods.rglob("*") if path.is_file()}
        result = run(args.babm, "bess-inspect", export, "--path", mods, "--json", as_json=True)
        assert result["selectable_variant"] and result["status"] == status, result
        after = {str(path.relative_to(mods)): digest(path) for path in mods.rglob("*") if path.is_file()}
        assert before == after, "Inspect changed the mods folder"
        return result

    def stage(work: Path, name: str, fixture: Path) -> Path:
        folder = work / "BESS-exports" / name
        folder.mkdir(parents=True)
        path = folder / "vehicle.zip"
        shutil.copy2(fixture, path)
        return path

    def setup(name: str) -> tuple[Path, Path, Path]:
        work = output / name
        mods = work / "mods"
        mods.mkdir(parents=True)
        original = mods / "original.zip"
        shutil.copy2(source, original)
        return work, mods, original

    def merge(mods: Path) -> tuple[Path, str]:
        run(args.babm, "merge", args.chassis, "--path", mods)
        packs = list(mods.glob("babm_*.zip"))
        assert len(packs) == 1, f"Merge did not produce one grouped pack: {packs}"
        slug = packs[0].stem.removeprefix("babm_")
        return packs[0], f"vehicles/{slug}/"

    def original_in_catalogue(mods: Path) -> None:
        if args.catalogue:
            catalogue = run(args.catalogue, mods, as_json=True)
            accessible = [Path(item["path"]) for item in catalogue["vehicles"]
                          if item["unavailable_reason"] is None]
            assert any(digest(path) == source_hash for path in accessible), "Original A missing in BESS"

    def preserve_grouped_original(target: Path, target_root: str) -> bool:
        # BABM regenerates family info.json. Its scanner labels an unknown
        # author as BESS / Automation when the group includes a BESS variant;
        # an explicit original Author, every other field and every other
        # original member must remain unchanged.
        info_path = target_root + "info.json"
        preserve(grouped_original, members(target), {info_path})
        expected = archive_json(control_pack, info_path)
        actual = archive_json(target, info_path)
        original_info = archive_json(source, first_marker["vehicle_root"] + "info.json")
        annotated = ("Author" not in original_info and expected.get("Author") == "Unknown"
                     and actual.get("Author") == "BESS / Automation")
        if annotated:
            actual["Author"] = expected["Author"]
        assert actual == expected, "Grouped original metadata changed beyond the absent-author annotation"
        return annotated

    def apply(mods: Path, target: Path, export: Path) -> dict:
        inspect(mods, export, "variant_ready")
        before = digest(target)
        receipt = run(args.babm, "bess-apply", export, "--path", mods, "--json", as_json=True)
        assert Path(receipt["target_path"]).resolve() == target.resolve(), receipt
        backup = Path(receipt["backup_path"]).resolve()
        assert backup.is_relative_to(mods.resolve()) and digest(backup) == before
        inspect(mods, export, "variant_installed")
        return receipt

    def refused(mods: Path, target: Path, export: Path, status: str) -> None:
        inspect(mods, export, status)
        before = digest(target)
        run(args.babm, "bess-apply", export, "--path", mods, "--json", failure=True)
        assert digest(target) == before, "Refused import changed the target"

    def update_and_check(work: Path, mods: Path, target: Path, target_root: str | None,
                         first_export: Path) -> dict:
        before = members(target)
        added_other = stage(work, "other", other) if other.is_file() else None
        if added_other:
            apply(mods, target, added_other)
            preserve(before, members(target), BOOKKEEPING)
            verify_payload(target, added_other, target_root)
        before_revision = members(target)
        next_export = stage(work, "revised", revised)
        receipt = apply(mods, target, next_export)
        after = members(target)
        previous_owned = owned(marker(first_export), target_root)
        revised_owned = owned(marker(next_export), target_root)
        preserve(before_revision, after, previous_owned | BOOKKEEPING)
        assert previous_owned - revised_owned <= before_revision.keys() - after.keys()
        assert set(after) - set(before_revision) <= revised_owned | BOOKKEEPING
        routing = verify_payload(target, next_export, target_root)
        refused(mods, target, first_export, "stale")
        refused(mods, target, next_export, "variant_installed")
        original_in_catalogue(mods)
        return {"revised_receipt": receipt, "routing": routing,
                "other_profile_preserved": added_other is not None,
                "obsolete_owned_files_removed": len(previous_owned - revised_owned),
                "unrelated_members_unchanged": True, "repeated_and_stale_import_refused": True}

    try:
        original_members = members(source)
        first_marker, revised_marker = marker(first), marker(revised)
        assert first_marker["variant_id"] == revised_marker["variant_id"]
        assert first_marker["exported_at_unix_ms"] < revised_marker["exported_at_unix_ms"]
        assert first_marker["added_files"] != revised_marker["added_files"], "Revision fixture did not change"
        for fixture in inputs[1:]:
            metadata = marker(fixture)
            assert metadata["source_archive_sha256"] == source_hash
            current = members(fixture)
            preserve(original_members, current)
            assert owned(metadata).isdisjoint(original_members)
            assert set(current) - set(original_members) == owned(metadata) | {MARKER}
            verify_routing(fixture, metadata, None)
        if other.is_file():
            assert marker(other)["variant_id"] != first_marker["variant_id"]
            assert owned(marker(other)).isdisjoint(owned(first_marker) | owned(revised_marker))

        # Pure-original grouping supplies the allowed grouping transformations
        # (unified info/core and JBeam display tags), independent of BESS import.
        _, control_mods, _ = setup("original-group-control")
        control_pack, grouped_root = merge(control_mods)
        grouped_original = members(control_pack)

        for grouped in (False, True):
            work, mods, original = setup("grouped-import" if grouped else "standalone-import")
            target, target_root = merge(mods) if grouped else (original, None)
            baseline = members(target)
            export = stage(work, "first", first)
            before_scan = digest(target)
            found = run(args.babm, "bess-scan", "--path", mods, "--exports", work / "BESS-exports",
                        "--json", as_json=True)
            assert any(item["selectable_variant"] and item["status"] == "variant_ready" for item in found)
            assert digest(target) == before_scan, "Discovery changed the target"
            receipt = apply(mods, target, export)
            preserve(baseline, members(target))
            assert set(members(target)) - set(baseline) == owned(first_marker, target_root) | BOOKKEEPING.intersection(members(target))
            verify_payload(target, export, target_root)
            refused(mods, target, export, "variant_installed")
            result = update_and_check(work, mods, target, target_root, export)
            result.update(scenario=work.name, first_receipt=receipt, original_members_unchanged=True)
            if grouped:
                run(args.babm, "unmerge", args.chassis, "--path", mods)
                assert not target.exists() and digest(original) == source_hash
                result["unmerge_restored_pristine_original"] = True
            report["scenarios"].append(result)
            save_report()

        # A full export placed beside the untouched original must contribute
        # only one extra configuration per profile, never a duplicate original.
        work, mods, original = setup("initial-merge-original-and-full")
        full = mods / "full-variant.zip"
        shutil.copy2(first, full)
        initial_inputs = {original: digest(original), full: digest(full)}
        if other.is_file():
            second_full = mods / "other-profile.zip"
            shutil.copy2(other, second_full)
            initial_inputs[second_full] = digest(second_full)
        target, target_root = merge(mods)
        assert target_root == grouped_root
        author_annotated = preserve_grouped_original(target, target_root)
        expected_configs = {name for name in grouped_original if name.endswith(".pc")}
        expected_configs.add(mapped(first_marker["configuration_path"], first_marker, target_root))
        verify_payload(target, first, target_root, merged=True)
        if other.is_file():
            second_marker = marker(other)
            expected_configs.add(mapped(second_marker["configuration_path"], second_marker, target_root))
            verify_payload(target, other, target_root, merged=True)
        assert {name for name in members(target) if name.endswith(".pc")} == expected_configs
        export = stage(work, "first", first)
        refused(mods, target, export, "variant_installed")
        before_revision = members(target)
        next_export = stage(work, "revised", revised)
        apply(mods, target, next_export)
        preserve(before_revision, members(target), owned(first_marker, target_root) | BOOKKEEPING)
        verify_payload(target, next_export, target_root)
        refused(mods, target, export, "stale")
        original_in_catalogue(mods)
        run(args.babm, "unmerge", args.chassis, "--path", mods)
        assert not target.exists()
        assert all(path.is_file() and digest(path) == expected for path, expected in initial_inputs.items())
        report["scenarios"].append({"scenario": work.name, "original_not_duplicated": True,
                                    "configurations": len(expected_configs), "revision_after_merge": True,
                                    "missing_author_annotated_as_bess": author_annotated,
                                    "unmerge_restored_all_inputs": True})
        save_report()

        # Import into a standalone archive before a later ordinary BABM merge.
        work, mods, original = setup("import-before-merge")
        export = stage(work, "first", first)
        apply(mods, original, export)
        imported_hash = digest(original)
        target, target_root = merge(mods)
        author_annotated = preserve_grouped_original(target, target_root)
        verify_payload(target, first, target_root, merged=True)
        result = update_and_check(work, mods, target, target_root, export)
        run(args.babm, "unmerge", args.chassis, "--path", mods)
        assert not target.exists() and digest(original) == imported_hash
        preserve(original_members, members(original))
        original_in_catalogue(mods)
        result.update(scenario=work.name, unmerge_restored_premerge_archive=True,
                      missing_author_annotated_as_bess=author_annotated,
                      original_configuration_and_audio_unchanged=True)
        report["scenarios"].append(result)

        # A complete archive that altered the retained original must be refused
        # before creating a merged pack or disabling either input. Alter only a
        # disposable copy; all declared BESS additions keep their valid hashes.
        work, mods, original = setup("reject-modified-original")
        corrupt = mods / "modified-original-in-full.zip"
        original_sound = next(name for name in original_members if name.endswith(".wav"))
        with zipfile.ZipFile(first) as archive, zipfile.ZipFile(corrupt, "x") as writer:
            for entry in archive.infolist():
                data = archive.read(entry.filename)
                if entry.filename == original_sound:
                    data += b"altered-original-fixture"
                writer.writestr(entry, data)
        before_refusal = {path.name: digest(path) for path in mods.iterdir() if path.is_file()}
        # The legacy merge CLI reports its error on stderr and can exit zero;
        # inspect the resulting filesystem rather than infer success from code.
        run(args.babm, "merge", args.chassis, "--path", mods)
        assert before_refusal == {path.name: digest(path) for path in mods.iterdir() if path.is_file()}
        assert not list(mods.glob("babm_*.zip")) and not (mods / ".babm_backup").exists()
        report["scenarios"].append({"scenario": work.name, "refused_without_mutation": True})
        assert all(digest(Path(path)) == expected for path, expected in source_hashes.items())
        report["coverage_notes"] = [
            "Owned-file deletion is checked when fixture inventories differ; otherwise the count is zero.",
            "An absent optional other-profile.zip or --catalogue is reported as unchecked, not a pass.",
        ]
        report["passed"] = True
    except Exception as error:
        report["error"] = f"{type(error).__name__}: {error}"
        raise
    finally:
        save_report()
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
