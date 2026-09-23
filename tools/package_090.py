"""Build the portable BESS 0.9.0 release from the verified R6 add-ons.

The twelve add-on ZIP payloads are copied unchanged. Their adjacent manifest and
project files are made portable, without claiming that the audio was regenerated.
"""

from __future__ import annotations

from html import escape
from pathlib import Path, PureWindowsPath
import hashlib
import io
import json
import shutil
import tomllib
import wave
import zipfile


ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "dist"
FOLDER = DIST / "BESS-0.9.0-public"
ARCHIVE = DIST / "BESS-0.9.0-Windows-Portable.zip"
STANDALONE = DIST / "BESS-0.9.0-Windows.exe"
SIDECAR = DIST / "BESS-0.9.0-Windows-Portable.zip.sha256"
EXE_SIDECAR = DIST / "BESS-0.9.0-Windows.exe.sha256"
FLEET = ROOT / "output/all-vehicles-mechanical-r6-20260923"
CERBERUS = ROOT / "output/cerberus-mechanical-prototype-r6-20260923"
SOURCE_DIR = ROOT / "cars"


def sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def source_name(raw: str) -> str:
    return PureWindowsPath(raw).name


def check_zip(path: Path) -> set[str]:
    with zipfile.ZipFile(path) as archive:
        bad = archive.testzip()
        assert bad is None, f"CRC failure: {path}: {bad}"
        names = archive.namelist()
        assert len(names) == len(set(names)), f"Duplicate ZIP members: {path}"
        return set(names)


def check_addon(source: Path, addon: Path, manifest: dict) -> int:
    assert manifest["kind"] == "configuration_addon", addon
    source_names = check_zip(source)
    with zipfile.ZipFile(addon) as archive:
        bad = archive.testzip()
        assert bad is None, f"CRC failure: {addon}: {bad}"
        names = archive.namelist()
        assert len(names) == len(set(names)), addon
        name_set = set(names)
        assert not (name_set & source_names), f"Add-on overwrites original: {addon}"
        required = {
            manifest[key]
            for key in (
                "config_path",
                "info_path",
                "engine_path",
                "blend_path",
                "engine_blend_path",
            )
        }
        assert required <= name_set, addon
        wav_names = manifest["wav_paths"] + manifest["engine_wav_paths"]
        assert len(wav_names) == len(set(wav_names)), addon
        assert set(wav_names) == {name for name in names if name.lower().endswith(".wav")}, addon
        assert len(manifest["wav_paths"]) == len(manifest["engine_wav_paths"]), addon
        for name in wav_names:
            with archive.open(name) as member:
                # BESS WAVs use the canonical PCM header, which fits in 256 bytes.
                with wave.open(io.BytesIO(member.read(256))) as wav:
                    actual = wav.getframerate(), wav.getsampwidth(), wav.getnchannels()
                    assert actual == (48000, 3, 1), (addon, name, actual)
        return len(wav_names)


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def start_page(rows: list[dict]) -> str:
    items = "\n".join(
        '<li><strong>{label}</strong> — <a href="sources/{source}">original ZIP</a> · '
        '<a href="beamng-addons/{vehicle}/{addon}">BESS add-on ZIP</a> · '
        '<a href="beamng-addons/{vehicle}/settings.bess.json">BESS project</a></li>'.format(
            label=escape(row["vehicle"]),
            source=escape(row["source_zip"]),
            vehicle=escape(row["vehicle_id"]),
            addon=escape(row["addon_zip"]),
        )
        for row in rows
    )
    return f'''<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>BESS 0.9.0 — Windows portable</title>
<style>body{{background:#10171f;color:#e6edf5;font:17px/1.55 system-ui;max-width:980px;margin:40px auto;padding:0 24px}}a{{color:#9ad3ff}}h2{{color:#8be5c6;margin-top:32px}}section{{background:#1b2633;padding:20px 24px;border-radius:12px;margin:18px 0}}li{{margin:9px 0}}small{{color:#b7c3d1}}</style>
<h1>BESS 0.9.0</h1><p>Bunchy's Engine Synthesis System connects Automation engine sounds to selectable BeamNG.drive configurations.</p>
<section><h2>1. Start BESS</h2><p>Run <a href="BESS.exe">BESS.exe</a>. Import one of the original Automation ZIPs below, or open its included BESS project. Use <strong>Calculate BeamNG level</strong> before export to inspect the rendered file levels. This is an estimate of exported WAV levels, not a guaranteed in-game volume.</p></section>
<section><h2>2. Install a BeamNG configuration</h2><p>Keep the original Automation ZIP enabled. Add the corresponding BESS add-on ZIP to your active BeamNG mods folder. Remove any older BESS add-on or full-replacement ZIP for the same vehicle first. In BeamNG, select the original vehicle, then choose its trim with <strong>(BESS)</strong> appended. The original trim remains available.</p><ul>{items}</ul></section>
<section><h2>3. Check the result</h2><p>Compare the original and BESS configurations from the cockpit, hood and tailpipe cameras. The add-ons contain separate engine/intake and exhaust banks in mono 48 kHz / 24-bit PCM. The twelve R6 add-ons passed structural and media checks; their naturalness and in-game balance have not been confirmed by listening across the whole fleet.</p><p><a href="RELEASE_NOTES_0.9.0.md">Release notes</a> · <a href="README.md">Project guide</a> · <a href="beamng-addons/verification.json">Fleet verification</a> · <a href="SHA256.json">File hashes</a></p></section>
</html>'''


def main() -> None:
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
    assert version == "0.9.0", f"Build version is {version}, expected 0.9.0"
    binary = ROOT / "target/release/bess.exe"
    required = (binary, ROOT / "README.md", ROOT / "RELEASE_NOTES_0.9.0.md")
    for path in required:
        assert path.is_file(), path
    assert binary.stat().st_mtime_ns >= (ROOT / "Cargo.toml").stat().st_mtime_ns, "Release binary predates version bump"
    for path in (FOLDER, ARCHIVE, STANDALONE, SIDECAR, EXE_SIDECAR):
        assert not path.exists(), f"Refusing to overwrite existing release asset: {path}"

    audit = json.loads((FLEET / "verification.json").read_text(encoding="utf-8"))
    assert audit["status"] == "structurally_verified_not_listening_validated"
    assert len(audit["vehicles"]) == 12
    candidates = {p.name.removeprefix("bunchyearth23_"): p for p in FLEET.iterdir() if p.is_dir()}
    candidates["cerberus_a"] = CERBERUS
    assert len(candidates) == 12, candidates.keys()
    rows = []
    total_wavs = 0
    source_names = set()
    addon_names = set()
    for checked in audit["vehicles"]:
        source_zip = checked["source_zip"]
        addon_zip = checked["addon_zip"]
        source = SOURCE_DIR / source_zip
        vehicle_id = source_zip.removeprefix("bunchyearth23_").removesuffix(".zip")
        candidate = candidates[vehicle_id]
        addon = candidate / addon_zip
        manifest = json.loads((candidate / "manifest.json").read_text(encoding="utf-8"))
        project = json.loads((candidate / "settings.bess.json").read_text(encoding="utf-8"))
        assert source.is_file() and addon.is_file(), (source, addon)
        assert source_name(manifest["source_archive"]) == source_zip
        assert source_name(project["source"]["archive"]) == source_zip
        assert manifest["vehicle_id"] == vehicle_id
        assert manifest["zip_file"] == addon_zip
        assert manifest["display_name"] == checked["vehicle"]
        assert manifest["source_sha256"] == checked["source_sha256"] == sha256(source)
        assert checked["addon_sha256"] == sha256(addon)
        assert source_zip not in source_names and addon_zip not in addon_names
        source_names.add(source_zip)
        addon_names.add(addon_zip)
        wav_count = check_addon(source, addon, manifest)
        assert wav_count == checked["wav_count"]
        assert len(manifest["engine_wav_paths"]) == checked["engine_wav_count"]
        total_wavs += wav_count
        rows.append({
            "vehicle": checked["vehicle"], "vehicle_id": vehicle_id,
            "source_zip": source_zip, "source_sha256": checked["source_sha256"],
            "addon_zip": addon_zip, "addon_sha256": checked["addon_sha256"],
            "wav_count": wav_count,
        })
    assert len(rows) == 12

    FOLDER.mkdir()
    (FOLDER / "sources").mkdir()
    addon_folder = FOLDER / "beamng-addons"
    addon_folder.mkdir()
    shutil.copy2(binary, FOLDER / "BESS.exe")
    shutil.copy2(ROOT / "README.md", FOLDER / "README.md")
    shutil.copy2(ROOT / "RELEASE_NOTES_0.9.0.md", FOLDER / "RELEASE_NOTES_0.9.0.md")
    shutil.copytree(ROOT / "docs", FOLDER / "docs")
    for row in rows:
        vehicle_id = row["vehicle_id"]
        candidate = candidates[vehicle_id]
        target = addon_folder / vehicle_id
        target.mkdir()
        shutil.copy2(SOURCE_DIR / row["source_zip"], FOLDER / "sources" / row["source_zip"])
        shutil.copy2(candidate / row["addon_zip"], target / row["addon_zip"])
        shutil.copy2(candidate / "INSTALLATION.txt", target / "INSTALLATION.txt")
        manifest = json.loads((candidate / "manifest.json").read_text(encoding="utf-8"))
        manifest["original_export_version"] = manifest["version"]
        manifest["version"] = "0.9.0"
        manifest["source_archive"] = f"../../sources/{row['source_zip']}"
        manifest["release_package"] = "BESS-0.9.0-Windows-Portable.zip"
        write_json(target / "manifest.json", manifest)
        project = json.loads((candidate / "settings.bess.json").read_text(encoding="utf-8"))
        project["source"]["archive"] = f"../../sources/{row['source_zip']}"
        write_json(target / "settings.bess.json", project)
    write_json(addon_folder / "verification.json", {
        "release": "0.9.0",
        "status": "structurally_verified_not_listening_validated",
        "origin": "R6 add-ons generated on 2026-09-23 and copied byte-for-byte",
        "vehicles": rows,
        "totals": {"vehicles": len(rows), "wav_files": total_wavs},
    })
    (FOLDER / "START.html").write_text(start_page(rows), encoding="utf-8")

    hashes = {p.relative_to(FOLDER).as_posix(): sha256(p) for p in sorted(FOLDER.rglob("*")) if p.is_file()}
    write_json(FOLDER / "SHA256.json", hashes)
    # All ZIP contents are already compressed; storing them avoids wasteful recompression.
    with zipfile.ZipFile(ARCHIVE, "w", allowZip64=True) as destination:
        for file in sorted(FOLDER.rglob("*")):
            if file.is_file():
                destination.write(file, f"{FOLDER.name}/{file.relative_to(FOLDER).as_posix()}", compress_type=zipfile.ZIP_STORED)
    shutil.copy2(binary, STANDALONE)
    SIDECAR.write_text(f"{sha256(ARCHIVE)}  {ARCHIVE.name}\n", encoding="ascii")
    EXE_SIDECAR.write_text(f"{sha256(STANDALONE)}  {STANDALONE.name}\n", encoding="ascii")
    print(f"{FOLDER}: {len(hashes)} files, {len(rows)} selectable add-ons, {total_wavs} PCM24 loops")
    print(f"{ARCHIVE}: {ARCHIVE.stat().st_size} bytes")
    print(f"{STANDALONE}: {STANDALONE.stat().st_size} bytes")


if __name__ == "__main__":
    main()
