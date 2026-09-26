"""Build the application and recording-free BESS 0.11.1 Windows packages.

Users import their own Automation exports and create their own selectable
BeamNG configurations. No vehicle, project or listening data are bundled.
"""

from __future__ import annotations

from pathlib import Path
import hashlib
import json
import shutil
import sys
import tomllib
import zipfile


ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "dist"
FOLDER = DIST / "BESS-0.11.1-public"
ARCHIVE = DIST / "BESS-0.11.1-Windows-Portable.zip"
STANDALONE = DIST / "BESS-0.11.1-Windows.exe"
SIDECAR = DIST / "BESS-0.11.1-Windows-Portable.zip.sha256"
EXE_SIDECAR = DIST / "BESS-0.11.1-Windows.exe.sha256"
INSTRUMENT_FOLDER = DIST / "BESS-0.11.1-Standalone-public"
INSTRUMENT_ARCHIVE = DIST / "BESS-0.11.1-Standalone-Windows-Portable.zip"
INSTRUMENT_SIDECAR = DIST / "BESS-0.11.1-Standalone-Windows-Portable.zip.sha256"
FILES = ("BESS.exe", "START.html", "README.md", "RELEASE_NOTES_0.11.1.md")
INSTRUMENT_FILES = (
    "standalone_engine.exe",
    "standalone_live.exe",
    "START_STANDALONE.md",
    "presets/single.json",
    "presets/four-even.json",
    "presets/four-split.json",
)


def sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def start_page() -> str:
    return '''<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>BESS 0.11.1 — Windows portable</title>
<style>body{background:#10171f;color:#e6edf5;font:17px/1.55 system-ui;max-width:880px;margin:40px auto;padding:0 24px}a{color:#9ad3ff}h2{color:#8be5c6;margin-top:32px}section{background:#1b2633;padding:20px 24px;border-radius:12px;margin:18px 0}</style>
<h1>BESS 0.11.1</h1><p>Bunchy's Engine Synthesis System connects Automation engine sounds to selectable BeamNG.drive configurations.</p>
<section><h2>1. Import your vehicle</h2><p>Run <a href="BESS.exe">BESS.exe</a> and select <strong>Import Automation ZIP</strong>. Choose a vehicle exported from your own Automation installation. BESS reads its RPM/load sound bank while leaving the original ZIP untouched.</p></section>
<section><h2>2. Shape and inspect the sound</h2><p>Compare Automation and BESS in live playback, adjust the sound controls, and use <strong>Calculate BeamNG level</strong> to inspect exported WAV levels. Preview the sound through in-bench BeamNG camera perspectives (Cockpit, Hood, Tailpipe, Orbit) and fine-tune idle gain.</p></section>
<section><h2>3. Export a selectable BeamNG configuration</h2><p>Choose <strong>Create BeamNG configuration</strong>. BESS writes an add-on ZIP for the imported vehicle. Keep the original Automation ZIP enabled and place the new add-on ZIP in your active BeamNG mods folder. In the vehicle selector, choose the original vehicle and its trim marked <strong>(BESS)</strong>. Remove an older BESS add-on for that same vehicle before enabling the new one.</p></section>
<p><a href="README.md">Project guide</a> · <a href="RELEASE_NOTES_0.11.1.md">Release notes</a> · <a href="SHA256.json">File hashes</a></p>
</html>'''


def portable_readme() -> str:
    return """# BESS 0.11.1 — Windows portable

Extract the complete ZIP, open [START.html](START.html), then run `BESS.exe`.
Import an Automation vehicle ZIP you own. New imports use standard source-guided
resynthesis. The experimental generated mode is a separate listening option.
Compare A and B, adjust levels and character, and use **Create BeamNG
configuration** to export a selectable add-on. Keep the original Automation ZIP
enabled beside the BESS add-on. Remove an older BESS add-on for the same vehicle.

The separate `BESS-0.11.1-Standalone-Windows-Portable.zip` contains the
recording-free engine instrument and generic presets. It is not required for
the main BESS workflow.

See the [release notes](RELEASE_NOTES_0.11.1.md) for changes and scope. The
portable folder contains no vehicle, prepared add-on or sound-bank data.
"""


def main() -> None:
    if sys.argv[1:] not in ([], ["--resume-instrument"]):
        raise SystemExit("Usage: package_111.py [--resume-instrument]")
    resume_instrument = sys.argv[1:] == ["--resume-instrument"]
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
    assert version == "0.11.1", f"Build version is {version}, expected 0.11.1"
    binary = ROOT / "target/release/bess.exe"
    for path in (binary, ROOT / "README.md", ROOT / "RELEASE_NOTES_0.11.1.md", ROOT / "START_STANDALONE.md"):
        assert path.is_file(), path
    with binary.open("rb") as stream:
        assert binary.stat().st_size > 1_000_000 and stream.read(2) == b"MZ", "Expected a Windows release executable"
    for name in ("standalone_engine", "standalone_live"):
        tool = ROOT / "target/release" / f"{name}.exe"
        with tool.open("rb") as stream:
            assert tool.stat().st_size > 100_000 and stream.read(2) == b"MZ", tool
    if resume_instrument:
        for path in (FOLDER, ARCHIVE, STANDALONE, SIDECAR, EXE_SIDECAR, INSTRUMENT_FOLDER):
            assert path.exists(), f"Missing partial package: {path}"
        assert not INSTRUMENT_ARCHIVE.exists() and not INSTRUMENT_SIDECAR.exists()
        assert sha256(FOLDER / "BESS.exe") == sha256(binary) == sha256(STANDALONE)
        assert (FOLDER / "README.md").read_text(encoding="utf-8") == portable_readme()
        assert sha256(FOLDER / "RELEASE_NOTES_0.11.1.md") == sha256(ROOT / "RELEASE_NOTES_0.11.1.md")
        assert SIDECAR.read_text(encoding="ascii") == f"{sha256(ARCHIVE)}  {ARCHIVE.name}\n"
        assert EXE_SIDECAR.read_text(encoding="ascii") == f"{sha256(STANDALONE)}  {STANDALONE.name}\n"
        with zipfile.ZipFile(ARCHIVE) as package:
            assert package.testzip() is None
        assert not any(path.is_file() for path in INSTRUMENT_FOLDER.rglob("*"))
    else:
        for path in (FOLDER, ARCHIVE, STANDALONE, SIDECAR, EXE_SIDECAR, INSTRUMENT_FOLDER, INSTRUMENT_ARCHIVE, INSTRUMENT_SIDECAR):
            assert not path.exists(), f"Refusing to overwrite existing release asset: {path}"
        FOLDER.mkdir()
        shutil.copy2(binary, FOLDER / "BESS.exe")
        (FOLDER / "README.md").write_text(portable_readme(), encoding="utf-8")
        shutil.copy2(ROOT / "RELEASE_NOTES_0.11.1.md", FOLDER / "RELEASE_NOTES_0.11.1.md")
        (FOLDER / "START.html").write_text(start_page(), encoding="utf-8")

        hashes = {name: sha256(FOLDER / name) for name in FILES}
        (FOLDER / "SHA256.json").write_text(json.dumps(hashes, indent=2) + "\n", encoding="utf-8")
        with zipfile.ZipFile(ARCHIVE, "w", allowZip64=True) as destination:
            for path in sorted(FOLDER.iterdir()):
                destination.write(path, f"{FOLDER.name}/{path.name}", compress_type=zipfile.ZIP_DEFLATED)
        shutil.copy2(binary, STANDALONE)
        SIDECAR.write_text(f"{sha256(ARCHIVE)}  {ARCHIVE.name}\n", encoding="ascii")
        EXE_SIDECAR.write_text(f"{sha256(STANDALONE)}  {STANDALONE.name}\n", encoding="ascii")
        INSTRUMENT_FOLDER.mkdir()
        (INSTRUMENT_FOLDER / "presets").mkdir()
    for name in ("standalone_engine", "standalone_live"):
        tool = ROOT / "target/release" / f"{name}.exe"
        shutil.copy2(tool, INSTRUMENT_FOLDER / f"{name}.exe")
    shutil.copy2(ROOT / "START_STANDALONE.md", INSTRUMENT_FOLDER / "START_STANDALONE.md")
    for name in ("single", "four-even", "four-split"):
        shutil.copy2(ROOT / "presets/standalone" / f"{name}.json", INSTRUMENT_FOLDER / "presets" / f"{name}.json")
    instrument_hashes = {name: sha256(INSTRUMENT_FOLDER / name) for name in INSTRUMENT_FILES}
    (INSTRUMENT_FOLDER / "SHA256.json").write_text(json.dumps(instrument_hashes, indent=2) + "\n", encoding="utf-8")
    with zipfile.ZipFile(INSTRUMENT_ARCHIVE, "w", allowZip64=True) as destination:
        for path in sorted(INSTRUMENT_FOLDER.rglob("*")):
            if path.is_file():
                destination.write(path, f"{INSTRUMENT_FOLDER.name}/{path.relative_to(INSTRUMENT_FOLDER).as_posix()}", compress_type=zipfile.ZIP_DEFLATED)
    INSTRUMENT_SIDECAR.write_text(f"{sha256(INSTRUMENT_ARCHIVE)}  {INSTRUMENT_ARCHIVE.name}\n", encoding="ascii")
    print(f"{FOLDER}: application and {len(FILES) - 1} English text files; no vehicle data")
    print(f"{ARCHIVE}: {ARCHIVE.stat().st_size} bytes")
    print(f"{STANDALONE}: {STANDALONE.stat().st_size} bytes")
    print(f"{INSTRUMENT_ARCHIVE}: {INSTRUMENT_ARCHIVE.stat().st_size} bytes; generic presets only")


if __name__ == "__main__":
    main()
