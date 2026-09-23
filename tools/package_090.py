"""Build the application-only BESS 0.9.0 Windows release.

Users import their own Automation exports and create their own selectable
BeamNG configurations. No vehicle, project or listening data are bundled.
"""

from __future__ import annotations

from pathlib import Path
import hashlib
import json
import shutil
import tomllib
import zipfile


ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "dist"
FOLDER = DIST / "BESS-0.9.0-public"
ARCHIVE = DIST / "BESS-0.9.0-Windows-Portable.zip"
STANDALONE = DIST / "BESS-0.9.0-Windows.exe"
SIDECAR = DIST / "BESS-0.9.0-Windows-Portable.zip.sha256"
EXE_SIDECAR = DIST / "BESS-0.9.0-Windows.exe.sha256"
FILES = ("BESS.exe", "START.html", "README.md", "RELEASE_NOTES_0.9.0.md")


def sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def start_page() -> str:
    return '''<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>BESS 0.9.0 — Windows portable</title>
<style>body{background:#10171f;color:#e6edf5;font:17px/1.55 system-ui;max-width:880px;margin:40px auto;padding:0 24px}a{color:#9ad3ff}h2{color:#8be5c6;margin-top:32px}section{background:#1b2633;padding:20px 24px;border-radius:12px;margin:18px 0}</style>
<h1>BESS 0.9.0</h1><p>Bunchy's Engine Synthesis System connects Automation engine sounds to selectable BeamNG.drive configurations.</p>
<section><h2>1. Import your vehicle</h2><p>Run <a href="BESS.exe">BESS.exe</a> and select <strong>Import Automation ZIP</strong>. Choose a vehicle exported from your own Automation installation. BESS reads its RPM/load sound bank while leaving the original ZIP untouched.</p></section>
<section><h2>2. Shape and inspect the sound</h2><p>Compare Automation and BESS in live playback, adjust the sound controls, and use <strong>Calculate BeamNG level</strong> to inspect exported WAV levels. The level panel cannot guarantee how loud BeamNG will sound because the game also applies vehicle gains, cabin filtering and spatial mixing.</p></section>
<section><h2>3. Export a selectable BeamNG configuration</h2><p>Choose <strong>Create BeamNG configuration</strong>. BESS writes an add-on ZIP for the imported vehicle. Keep the original Automation ZIP enabled and place the new add-on ZIP in your active BeamNG mods folder. In the vehicle selector, choose the original vehicle and its trim marked <strong>(BESS)</strong>. Remove an older BESS add-on for that same vehicle before enabling the new one.</p></section>
<p><a href="README.md">Project guide</a> · <a href="RELEASE_NOTES_0.9.0.md">Release notes</a> · <a href="SHA256.json">File hashes</a></p>
</html>'''


def main() -> None:
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
    assert version == "0.9.0", f"Build version is {version}, expected 0.9.0"
    binary = ROOT / "target/release/bess.exe"
    for path in (binary, ROOT / "README.md", ROOT / "RELEASE_NOTES_0.9.0.md"):
        assert path.is_file(), path
    with binary.open("rb") as stream:
        assert binary.stat().st_size > 1_000_000 and stream.read(2) == b"MZ", "Expected a Windows release executable"
    for path in (FOLDER, ARCHIVE, STANDALONE, SIDECAR, EXE_SIDECAR):
        assert not path.exists(), f"Refusing to overwrite existing release asset: {path}"

    FOLDER.mkdir()
    shutil.copy2(binary, FOLDER / "BESS.exe")
    shutil.copy2(ROOT / "README.md", FOLDER / "README.md")
    shutil.copy2(ROOT / "RELEASE_NOTES_0.9.0.md", FOLDER / "RELEASE_NOTES_0.9.0.md")
    (FOLDER / "START.html").write_text(start_page(), encoding="utf-8")

    hashes = {name: sha256(FOLDER / name) for name in FILES}
    (FOLDER / "SHA256.json").write_text(json.dumps(hashes, indent=2) + "\n", encoding="utf-8")
    with zipfile.ZipFile(ARCHIVE, "w", allowZip64=True) as destination:
        for path in sorted(FOLDER.iterdir()):
            destination.write(path, f"{FOLDER.name}/{path.name}", compress_type=zipfile.ZIP_DEFLATED)
    shutil.copy2(binary, STANDALONE)
    SIDECAR.write_text(f"{sha256(ARCHIVE)}  {ARCHIVE.name}\n", encoding="ascii")
    EXE_SIDECAR.write_text(f"{sha256(STANDALONE)}  {STANDALONE.name}\n", encoding="ascii")
    print(f"{FOLDER}: application and {len(FILES) - 1} English text files; no vehicle data")
    print(f"{ARCHIVE}: {ARCHIVE.stat().st_size} bytes")
    print(f"{STANDALONE}: {STANDALONE.stat().st_size} bytes")


if __name__ == "__main__":
    main()
