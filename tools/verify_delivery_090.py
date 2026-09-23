"""Verify the complete BESS 0.9.0 portable and standalone release assets."""

from __future__ import annotations

from html.parser import HTMLParser
from pathlib import Path
import hashlib
import io
import json
import re
import wave
import zipfile


ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "dist"
FOLDER = DIST / "BESS-0.9.0-public"
ARCHIVE = DIST / "BESS-0.9.0-Windows-Portable.zip"
STANDALONE = DIST / "BESS-0.9.0-Windows.exe"


def sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


class Links(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.urls: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        for key, value in attrs:
            if key in ("href", "src") and value and not value.startswith(("#", "http:", "https:", "data:")):
                self.urls.append(value.split("#", 1)[0])


def verify() -> None:
    assert (FOLDER / "BESS.exe").is_file()
    assert STANDALONE.is_file()
    assert sha256(FOLDER / "BESS.exe") == sha256(STANDALONE)
    for asset in (ARCHIVE, STANDALONE):
        sidecar = Path(str(asset) + ".sha256")
        assert sidecar.read_text(encoding="ascii") == f"{sha256(asset)}  {asset.name}\n"

    hashes = json.loads((FOLDER / "SHA256.json").read_text(encoding="utf-8"))
    actual = {p.relative_to(FOLDER).as_posix(): p for p in FOLDER.rglob("*") if p.is_file() and p.name != "SHA256.json"}
    assert set(actual) == set(hashes), "Portable manifest has missing or extra files"
    for relative, path in actual.items():
        assert sha256(path) == hashes[relative], relative

    page = Links()
    page.feed((FOLDER / "START.html").read_text(encoding="utf-8"))
    for url in page.urls:
        assert (FOLDER / url).is_file(), f"Broken START.html link: {url}"
    readme = (FOLDER / "README.md").read_text(encoding="utf-8")
    for url in re.findall(r"\]\(([^)]+)\)", readme):
        if not url.startswith(("http:", "https:", "#")):
            assert (FOLDER / url.split("#", 1)[0]).is_file(), f"Broken README.md link: {url}"

    audit = json.loads((FOLDER / "beamng-addons/verification.json").read_text(encoding="utf-8"))
    assert audit["release"] == "0.9.0"
    assert audit["status"] == "structurally_verified_not_listening_validated"
    rows = audit["vehicles"]
    assert len(rows) == audit["totals"]["vehicles"] == 12
    assert len({row["vehicle_id"] for row in rows}) == 12
    assert len({row["source_zip"] for row in rows}) == 12
    assert len({row["addon_zip"] for row in rows}) == 12
    wav_count = 0
    for row in rows:
        source = FOLDER / "sources" / row["source_zip"]
        addon_dir = FOLDER / "beamng-addons" / row["vehicle_id"]
        addon = addon_dir / row["addon_zip"]
        assert sha256(source) == row["source_sha256"]
        assert sha256(addon) == row["addon_sha256"]
        manifest = json.loads((addon_dir / "manifest.json").read_text(encoding="utf-8"))
        project = json.loads((addon_dir / "settings.bess.json").read_text(encoding="utf-8"))
        assert manifest["version"] == "0.9.0" and manifest["original_export_version"] == "0.8.4"
        assert manifest["kind"] == "configuration_addon"
        assert manifest["zip_file"] == row["addon_zip"]
        assert manifest["display_name"] == row["vehicle"]
        assert (addon_dir / manifest["source_archive"]).resolve() == source.resolve()
        assert (addon_dir / project["source"]["archive"]).resolve() == source.resolve()
        expected_wavs = set(manifest["wav_paths"] + manifest["engine_wav_paths"])
        assert len(expected_wavs) == row["wav_count"]
        assert len(manifest["wav_paths"]) == len(manifest["engine_wav_paths"])
        with zipfile.ZipFile(source) as source_archive, zipfile.ZipFile(addon) as addon_archive:
            assert source_archive.testzip() is None, source
            assert addon_archive.testzip() is None, addon
            source_names = source_archive.namelist()
            addon_names = addon_archive.namelist()
            assert len(source_names) == len(set(source_names))
            assert len(addon_names) == len(set(addon_names))
            assert not (set(source_names) & set(addon_names)), row["vehicle_id"]
            assert {name for name in addon_names if name.lower().endswith(".wav")} == expected_wavs
            for name in expected_wavs:
                with addon_archive.open(name) as stream:
                    with wave.open(io.BytesIO(stream.read(256))) as wav:
                        assert (wav.getframerate(), wav.getsampwidth(), wav.getnchannels()) == (48000, 3, 1), (addon, name)
        wav_count += len(expected_wavs)
    assert wav_count == audit["totals"]["wav_files"]

    with zipfile.ZipFile(ARCHIVE) as outer:
        assert outer.testzip() is None, "Portable archive CRC failure"
        names = outer.namelist()
        assert len(names) == len(set(names))
        assert set(names) == {f"{FOLDER.name}/{name}" for name in hashes} | {f"{FOLDER.name}/SHA256.json"}
        for relative, expected in hashes.items():
            with outer.open(f"{FOLDER.name}/{relative}") as stream:
                assert hashlib.sha256(stream.read()).hexdigest() == expected, relative
    print(f"PASS: 12 original ZIPs, 12 selectable BESS add-ons, {wav_count} mono 48 kHz / 24-bit loops")
    print(f"PASS: {len(hashes)} file hashes, {len(page.urls)} HTML links, outer ZIP CRC, standalone executable and sidecars")


if __name__ == "__main__":
    verify()
