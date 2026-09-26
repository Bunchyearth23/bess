"""Verify the BESS 0.11.1 application and recording-free Windows assets."""

from __future__ import annotations

from html.parser import HTMLParser
from pathlib import Path
import hashlib
import json
import re
import zipfile


ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "dist"
FOLDER = DIST / "BESS-0.11.1-public"
ARCHIVE = DIST / "BESS-0.11.1-Windows-Portable.zip"
STANDALONE = DIST / "BESS-0.11.1-Windows.exe"
INSTRUMENT_FOLDER = DIST / "BESS-0.11.1-Standalone-public"
INSTRUMENT_ARCHIVE = DIST / "BESS-0.11.1-Standalone-Windows-Portable.zip"
FILES = {"BESS.exe", "START.html", "README.md", "RELEASE_NOTES_0.11.1.md", "SHA256.json"}
INSTRUMENT_FILES = {
    "standalone_engine.exe",
    "standalone_live.exe",
    "START_STANDALONE.md",
    "presets/single.json",
    "presets/four-even.json",
    "presets/four-split.json",
    "SHA256.json",
}
FORBIDDEN_EXTENSIONS = {".zip", ".wav", ".car", ".bess.json"}
FORBIDDEN_DIRECTORIES = {"sources", "beamng-addons", "listening"}


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
    actual = {p.name: p for p in FOLDER.iterdir() if p.is_file()}
    assert set(actual) == FILES, f"Portable package has missing or extra files: {set(actual) ^ FILES}"
    assert all(p.is_file() for p in FOLDER.iterdir()), "Portable package has unexpected subdirectories"
    assert not any(name.lower().endswith(tuple(FORBIDDEN_EXTENSIONS)) for name in actual)
    assert not any(part.lower() in FORBIDDEN_DIRECTORIES for path in FOLDER.rglob("*") for part in path.parts)
    assert STANDALONE.is_file() and sha256(FOLDER / "BESS.exe") == sha256(STANDALONE)
    assert sha256(STANDALONE) == sha256(ROOT / "target/release/bess.exe"), "Release executable differs from verified build"
    for asset in (ARCHIVE, STANDALONE):
        sidecar = Path(str(asset) + ".sha256")
        assert sidecar.read_text(encoding="ascii") == f"{sha256(asset)}  {asset.name}\n"

    hashes = json.loads((FOLDER / "SHA256.json").read_text(encoding="utf-8"))
    assert set(hashes) == FILES - {"SHA256.json"}
    for name, expected in hashes.items():
        assert sha256(FOLDER / name) == expected, name

    page = Links()
    page.feed((FOLDER / "START.html").read_text(encoding="utf-8"))
    assert all((FOLDER / url).is_file() for url in page.urls), "Broken START.html link"
    readme = (FOLDER / "README.md").read_text(encoding="utf-8")
    for url in re.findall(r"\]\(([^)]+)\)", readme):
        if not url.startswith(("http:", "https:", "#")):
            assert (FOLDER / url.split("#", 1)[0]).is_file(), f"Broken README.md link: {url}"

    with zipfile.ZipFile(ARCHIVE) as outer:
        assert outer.testzip() is None, "Portable archive CRC failure"
        names = outer.namelist()
        expected = {f"{FOLDER.name}/{name}" for name in FILES}
        assert len(names) == len(set(names)) and set(names) == expected, "Unexpected ZIP members"
        for name, checksum in hashes.items():
            with outer.open(f"{FOLDER.name}/{name}") as stream:
                assert hashlib.sha256(stream.read()).hexdigest() == checksum, name
        with outer.open(f"{FOLDER.name}/SHA256.json") as stream:
            assert hashlib.sha256(stream.read()).hexdigest() == sha256(FOLDER / "SHA256.json")
    actual_instrument = {
        path.relative_to(INSTRUMENT_FOLDER).as_posix(): path
        for path in INSTRUMENT_FOLDER.rglob("*") if path.is_file()
    }
    assert set(actual_instrument) == INSTRUMENT_FILES, "Instrument has missing or extra files"
    assert not any(name.lower().endswith(tuple(FORBIDDEN_EXTENSIONS)) for name in actual_instrument)
    for name in ("standalone_engine", "standalone_live"):
        assert sha256(INSTRUMENT_FOLDER / f"{name}.exe") == sha256(ROOT / "target/release" / f"{name}.exe")
    instrument_hashes = json.loads((INSTRUMENT_FOLDER / "SHA256.json").read_text(encoding="utf-8"))
    assert set(instrument_hashes) == INSTRUMENT_FILES - {"SHA256.json"}
    for name, expected in instrument_hashes.items():
        assert sha256(INSTRUMENT_FOLDER / name) == expected, name
    with zipfile.ZipFile(INSTRUMENT_ARCHIVE) as outer:
        assert outer.testzip() is None, "Instrument archive CRC failure"
        expected = {f"{INSTRUMENT_FOLDER.name}/{name}" for name in INSTRUMENT_FILES}
        assert len(outer.namelist()) == len(expected) and set(outer.namelist()) == expected
        for name, checksum in instrument_hashes.items():
            with outer.open(f"{INSTRUMENT_FOLDER.name}/{name}") as stream:
                assert hashlib.sha256(stream.read()).hexdigest() == checksum, name
    sidecar = Path(str(INSTRUMENT_ARCHIVE) + ".sha256")
    assert sidecar.read_text(encoding="ascii") == f"{sha256(INSTRUMENT_ARCHIVE)}  {INSTRUMENT_ARCHIVE.name}\n"
    print("PASS: application package has five allow-listed files; instrument package has seven")
    print(f"PASS: {len(hashes) + len(instrument_hashes)} file hashes, {len(page.urls)} HTML links, both ZIP CRCs, executables and sidecars")


if __name__ == "__main__":
    verify()
