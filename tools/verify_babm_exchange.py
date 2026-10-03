"""Exercise the compiled BESS/BABM roundtrip on disposable, source-bound fixtures.

Create fixture inputs with BESS_BABM_TEST_ARTIFACTS set while running the
beamng_full_export integration test. This script never touches installed mods.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import zipfile
from pathlib import Path


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def members(path: Path) -> dict[str, str]:
    with zipfile.ZipFile(path) as archive:
        assert len(archive.namelist()) == len(set(archive.namelist()))
        result = {}
        for entry in archive.infolist():
            with archive.open(entry) as stream:
                result[entry.filename] = hashlib.file_digest(stream, "sha256").hexdigest()
        return result


def marker(path: Path) -> dict:
    with zipfile.ZipFile(path) as archive:
        return json.loads(archive.read("bess-export.json"))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bess", type=Path, required=True)
    parser.add_argument("--babm", type=Path, required=True)
    parser.add_argument("--catalogue", type=Path, required=True)
    parser.add_argument("--fixtures", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir()  # Exclusive; never overwrite a previous proof.
    log = []

    def run(executable: Path, *arguments: object, json_output: bool = False,
            allow_failure: bool = False):
        command = [str(executable.resolve()), *(str(argument) for argument in arguments)]
        process = subprocess.run(command, capture_output=True, text=True,
                                 encoding="utf-8-sig", errors="replace", timeout=300)
        log.append({"command": command, "exit_code": process.returncode,
                    "stdout": process.stdout, "stderr": process.stderr})
        (output / "commands.json").write_text(json.dumps(log, indent=2), encoding="utf-8")
        if not allow_failure and process.returncode:
            raise RuntimeError(f"Command failed: {command}\n{process.stderr}\n{process.stdout}")
        return json.loads(process.stdout) if json_output and process.returncode == 0 else process

    source = args.fixtures.resolve() / "original.zip"
    source_hash = digest(source)
    first = args.fixtures.resolve() / "exports/first.zip"
    revised = args.fixtures.resolve() / "exports/revised.zip"
    assert marker(first)["source_archive_sha256"] == source_hash
    assert marker(revised)["source_archive_sha256"] == source_hash
    results = []
    for grouped in (False, True):
        work = output / ("grouped" if grouped else "standalone")
        mods = work / "mods"
        mods.mkdir(parents=True)
        original = mods / "original.zip"
        shutil.copy2(source, original)
        if grouped:
            run(args.babm, "merge", "Test Vehicle", "--path", mods)
            target = mods / "babm_test_vehicle.zip"
        else:
            target = original
        assert target.is_file(), target
        baseline = members(target)
        exports = work / "BESS-exports"
        (exports / "first").mkdir(parents=True)
        export = exports / "first/vehicle.zip"
        shutil.copy2(first, export)

        def inspect(path: Path) -> dict:
            return run(args.babm, "bess-inspect", path, "--path", mods, "--json", json_output=True)

        def check_applied(path: Path) -> None:
            metadata = marker(path)
            current = members(target)
            sounds = {}
            for sound in metadata["sounds"]:
                name = sound["path"]
                if grouped and name.startswith(metadata["vehicle_root"]):
                    name = "vehicles/test_vehicle/" + name[len(metadata["vehicle_root"]):]
                sounds[name] = sound["rendered_sha256"]
            for name, old_hash in baseline.items():
                assert current[name] == sounds.get(name, old_hash), name
            assert set(current) - set(baseline) <= {"bess-export.json", "babm-bess-updates.json"}

        before_scan = digest(target)
        found = run(args.babm, "bess-scan", "--path", mods, "--exports", exports,
                    "--json", json_output=True)
        assert any(item["status"] == "ready" for item in found), found
        assert digest(target) == before_scan, "Read-only scan changed the mod"
        assert inspect(export)["status"] == "ready"
        first_receipt = run(args.babm, "bess-apply", export, "--path", mods, "--json", json_output=True)
        check_applied(export)
        assert inspect(export)["status"] == "already_applied"
        applied_hash = digest(target)
        run(args.babm, "bess-apply", export, "--path", mods, "--json", allow_failure=True)
        assert digest(target) == applied_hash, "Repeated apply changed the mod"

        catalogue = run(args.catalogue, mods, json_output=True)
        originals = [Path(vehicle["path"]) for vehicle in catalogue["vehicles"]
                     if vehicle["unavailable_reason"] is None]
        assert any(digest(path) == source_hash for path in originals), "Original A lost from BESS library"

        (exports / "revised").mkdir()
        next_export = exports / "revised/vehicle.zip"
        shutil.copy2(revised, next_export)
        assert inspect(next_export)["status"] == "ready"
        run(args.babm, "bess-apply", next_export, "--path", mods, "--json", json_output=True)
        check_applied(next_export)
        revised_hash = digest(target)
        run(args.babm, "bess-apply", export, "--path", mods, "--json", allow_failure=True)
        assert digest(target) == revised_hash, "Older export rolled the mod back"

        if not grouped:
            # Exercise real BESS reopening the ZIP written by BABM, rather than
            # merely comparing two separately produced BESS exports.
            third_dir = exports / "reopened"
            run(args.bess, "--beamng-replacement", target, third_dir)
            third = next(third_dir.glob("*.zip"))
            assert marker(third)["source_archive_sha256"] == source_hash
            run(args.babm, "bess-apply", third, "--path", mods, "--json", json_output=True)
            check_applied(third)
        else:
            run(args.babm, "unmerge", "Test Vehicle", "--path", mods)
            assert digest(original) == source_hash, "Unmerge did not restore untouched original"
        assert digest(source) == source_hash
        results.append({"scenario": work.name, "source_unchanged": True,
                        "non_audio_members_unchanged": True, "first_receipt": first_receipt,
                        "original_available_in_bess": True, "repeated_and_stale_apply_safe": True})
    receipt = {"bess_sha256": digest(args.bess), "babm_sha256": digest(args.babm),
               "scenarios": results}
    (output / "verification.json").write_text(json.dumps(receipt, indent=2), encoding="utf-8")
    print(json.dumps(receipt, indent=2))


if __name__ == "__main__":
    main()
