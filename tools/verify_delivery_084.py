"""Check links, source paths, PCM24 media and hashes in the BESS 0.8.4 delivery."""
from html.parser import HTMLParser
from pathlib import Path
import hashlib
import io
import json
import wave
import zipfile

root = Path(__file__).resolve().parents[1]
out = root / "dist/BESS-0.8.4-public"
assert (out / "BESS.exe").is_file()
assert (out / "documentation/reports/CERBERUS-5200-0.8.4-2026-09-22.md").is_file()
corpus = json.loads((out / "listening/corpus.json").read_text(encoding="utf-8"))
assert len(corpus["vehicles"]) == 12
fingerprints = {Path(row["archive"]).name: row["source"]["fingerprint"] for row in corpus["vehicles"]}


class Links(HTMLParser):
    def __init__(self):
        super().__init__()
        self.urls = []

    def handle_starttag(self, tag, attrs):
        values = dict(attrs)
        for key in ("href", "src"):
            if key in values and not values[key].startswith(("#", "http:", "https:", "data:")):
                self.urls.append(values[key].split("#", 1)[0])


links = 0
for html in (out / "START.html", out / "CERBERUS-5200.html", out / "listening/index.html"):
    parser = Links()
    parser.feed(html.read_text(encoding="utf-8"))
    for url in parser.urls:
        assert (html.parent / url).is_file(), (html, url)
        links += 1

projects = list(out.rglob("*.bess.json"))
assert len(projects) == 36
for path in projects:
    project = json.loads(path.read_text(encoding="utf-8"))
    source = project["source"]
    archive = (path.parent / source["archive"]).resolve()
    assert archive.is_file() and out.resolve() in archive.parents
    assert source["fingerprint"] == fingerprints[archive.name]

comparisons = 0
for row in corpus["vehicles"]:
    path = out / "listening" / row["folder"]
    for name in ("01-source-automation.wav", "02-bess-enhanced.wav", "03-bess-previous.wav"):
        wav_path = path / name
        assert wav_path.is_file(), wav_path
        with wave.open(str(wav_path)) as wav:
            assert (wav.getframerate(), wav.getsampwidth(), wav.getnchannels()) == (48000, 3, 1), wav_path
        comparisons += 1
assert comparisons == 36

steady = out / "cerberus-5200"
expected_steady = {
    f"{load}/{name}"
    for load in ("full", "light")
    for name in ("01-source.wav", "02-bess-0.8.4.wav", "03-bess-0.8.3.wav")
}
assert {path.relative_to(steady).as_posix() for path in steady.rglob("*.wav")} == expected_steady
for relative in expected_steady:
    path = steady / relative
    with wave.open(str(path)) as wav:
        assert (wav.getframerate(), wav.getsampwidth(), wav.getnchannels()) == (48000, 3, 1), path

replaced = 0
manifests = list((out / "beamng").rglob("manifest.json"))
assert len(manifests) == 12
for manifest_path in manifests:
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    with zipfile.ZipFile(manifest_path.parent / manifest["zip_file"]) as archive:
        for loop in manifest["loops"]:
            with wave.open(io.BytesIO(archive.read(loop["path"]))) as wav:
                assert (wav.getframerate(), wav.getsampwidth(), wav.getnchannels()) == (48000, 3, 1), (manifest_path, loop["path"])
            replaced += 1
assert replaced == 688

hashes = json.loads((out / "SHA256.json").read_text(encoding="utf-8"))
actual = [path for path in out.rglob("*") if path.is_file() and path.name != "SHA256.json"]
assert len(actual) == len(hashes)
for path in actual:
    relative = path.relative_to(out).as_posix()
    with path.open("rb") as stream:
        assert hashlib.file_digest(stream, "sha256").hexdigest() == hashes[relative], relative

print(f"{len(actual)} hashes, {links} links, {len(projects)} portable projects, {comparisons} comparison WAVs, {len(expected_steady)} Cerberus WAVs and {replaced} BeamNG WAVs in 48 kHz/24-bit")
