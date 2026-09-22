"""Create the portable BESS 0.8.4 delivery from the verified PCM24 outputs."""
from pathlib import Path
import hashlib
import html
import json
import os
import shutil
import wave

root = Path(__file__).resolve().parents[1]
out = root / "dist/BESS-0.8.4-public"
binary = root / "dist/BESS-0.8.4-English.exe"
corpus_dir = root / "output/corpus-0.8.4-en"
beamng_dir = root / "output/beamng-0.8.4-en"
report = root / "docs/reports/CERBERUS-5200-0.8.4-2026-09-22.md"
steady = {}
for load in ("full", "light"):
    previous = root / f"output/steady-0.8.3-cerberus-5200-{load}"
    current = root / f"output/steady-0.8.4-cerberus-5200-en-{load}"
    steady[load] = {
        "source_previous": previous / "source.wav",
        "source_current": current / "source.wav",
        "previous": previous / "bess.wav",
        "current": current / "bess.wav",
    }

for required in (binary, corpus_dir / "corpus.json", corpus_dir / "index.html",
                 beamng_dir / "verification.json", report,
                 *(path for inputs in steady.values() for path in inputs.values())):
    assert required.is_file(), required

for inputs in steady.values():
    with inputs["source_previous"].open("rb") as stream:
        previous_hash = hashlib.file_digest(stream, "sha256").digest()
    with inputs["source_current"].open("rb") as stream:
        assert hashlib.file_digest(stream, "sha256").digest() == previous_hash
    for path in inputs.values():
        with wave.open(str(path)) as wav:
            assert (wav.getframerate(), wav.getsampwidth(), wav.getnchannels()) == (48000, 3, 1), path

verification = json.loads((beamng_dir / "verification.json").read_text(encoding="utf-8"))
corpus = json.loads((corpus_dir / "corpus.json").read_text(encoding="utf-8"))
assert len(verification) == len(corpus["vehicles"]) == 12
assert all(row["status"] == "ok" for row in corpus["vehicles"])
assert all(row["source_unchanged"] for row in verification)
assert sum(row["loops"] for row in verification) == 688
assert all(row["metadata"]["automation_engine"] for row in corpus["vehicles"])

out.mkdir()  # Reject accidental overwrite of an existing delivery.
shutil.copy2(binary, out / "BESS.exe")
for source, target in [
    (root / "cars", out / "sources"),
    (corpus_dir, out / "listening"),
    (beamng_dir, out / "beamng"),
]:
    shutil.copytree(source, target)
shutil.copytree(
    root / "docs",
    out / "documentation",
    ignore=shutil.ignore_patterns(
        "*.png",
        "acoustics-0.3-metrics.json",
        "drive-metrics.json",
        "hybrid-comparison-metrics.json",
        "FORMAT-0.8.3-metrics.json",
        "LIVE-48K-0.8.3-WAV-hashes.json",
    ),
)
shutil.copy2(root / "README.md", out / "README.md")
shutil.copy2(root / "RELEASE_NOTES_0.8.4.md", out / "RELEASE_NOTES_0.8.4.md")
steady_out = out / "cerberus-5200"
for load, inputs in steady.items():
    target = steady_out / load
    target.mkdir(parents=True)
    for source, name in (("source_current", "01-source.wav"),
                         ("current", "02-bess-0.8.4.wav"),
                         ("previous", "03-bess-0.8.3.wav")):
        shutil.copy2(inputs[source], target / name)

projects = list(out.rglob("*.bess.json"))
assert len(projects) == 36
for path in projects:
    project = json.loads(path.read_text(encoding="utf-8"))
    if project.get("source"):
        archive = Path(project["source"]["archive"]).name
        source = out / "sources" / archive
        assert source.is_file(), archive
        project["source"]["archive"] = Path(os.path.relpath(source, path.parent)).as_posix()
        path.write_text(json.dumps(project, ensure_ascii=False, indent=2), encoding="utf-8")

packaged_corpus = out / "listening/corpus.json"
corpus = json.loads(packaged_corpus.read_text(encoding="utf-8"))
for row in corpus["vehicles"]:
    archive = Path(row["source"]["archive"]).name
    assert (out / "sources" / archive).is_file()
    row["source"]["archive"] = f"../sources/{archive}"
packaged_corpus.write_text(json.dumps(corpus, ensure_ascii=False, indent=2), encoding="utf-8")

packaged_verification = out / "beamng/verification.json"
verification = json.loads(packaged_verification.read_text(encoding="utf-8"))
for row in verification:
    archive = Path(row["archive"]).name
    assert (out / "sources" / archive).is_file()
    row["archive"] = f"sources/{archive}"
packaged_verification.write_text(
    json.dumps(verification, ensure_ascii=False, indent=2), encoding="utf-8"
)

mods = []
zip_names = set()
for path in sorted((out / "beamng").iterdir()):
    if not path.is_dir():
        continue
    manifest = json.loads((path / "manifest.json").read_text(encoding="utf-8"))
    archive = Path(manifest["source"]["archive"]).name
    assert (out / "sources" / archive).is_file()
    manifest["source"]["archive"] = Path(os.path.relpath(
        out / "sources" / archive, path
    )).as_posix()
    (path / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    zip_name = manifest["zip_file"]
    assert (path / zip_name).is_file()
    assert zip_name not in zip_names
    zip_names.add(zip_name)
    label = html.escape(path.name.removeprefix("bunchyearth23_").replace("_", " "))
    mods.append(f'<li>{label} — <a href="beamng/{path.name}/{html.escape(zip_name)}">BeamNG copy</a> · <a href="beamng/{path.name}/INSTALLATION.txt">installation guide</a></li>')
assert len(mods) == 12

steady_rows = []
for load, label in (("full", "Full load"), ("light", "Light load")):
    cells = "".join(
        f'<td><audio controls preload="none" src="cerberus-5200/{load}/{name}"></audio><br><a href="cerberus-5200/{load}/{name}">Download</a></td>'
        for name in ("01-source.wav", "02-bess-0.8.4.wav", "03-bess-0.8.3.wav")
    )
    steady_rows.append(f"<tr><th>{label}</th>{cells}</tr>")

steady_page = f'''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Cerberus — 5,200 RPM — BESS 0.8.4</title>
<style>body{{background:#10171f;color:#e6edf5;font:17px/1.6 system-ui;max-width:1100px;margin:40px auto;padding:0 24px}}a{{color:#9ad3ff}}table{{border-collapse:collapse;width:100%}}td,th{{padding:16px;border:1px solid #496073;text-align:left;vertical-align:top}}audio{{max-width:100%;width:270px}}tr:nth-child(even){{background:#1b2633}}@media(max-width:800px){{table{{display:block;overflow-x:auto}}}}</style>
<p><a href="START.html">← Home</a></p><h1>Cerberus at 5,200 RPM</h1><p>Listen to full and light load separately. A is the source, B is the BESS 0.8.4 render, and C is the BESS 0.8.3 render. All six files are mono 48 kHz / 24-bit.</p>
<table><thead><tr><th>Load</th><th>A — Source</th><th>B — BESS 0.8.4</th><th>C — BESS 0.8.3</th></tr></thead><tbody>{''.join(steady_rows)}</tbody></table>
<p><a href="documentation/reports/CERBERUS-5200-0.8.4-2026-09-22.md">Measurements and limitations</a></p></html>'''
(out / "CERBERUS-5200.html").write_text(steady_page, encoding="utf-8")

page = f'''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>BESS 0.8.4</title>
<style>body{{background:#10171f;color:#e6edf5;font:17px/1.6 system-ui;max-width:980px;margin:40px auto;padding:0 24px}}h1{{font-size:40px}}h2{{margin-top:36px;color:#8be5c6}}a{{color:#9ad3ff}}section{{background:#1b2633;padding:22px;border-radius:12px;margin:18px 0}}li{{margin:8px 0}}small{{color:#b7c3d1}}</style>
<h1>BESS 0.8.4 — Cerberus at 5,200 RPM</h1><p>Application, twelve Automation sources, sound comparisons, and BeamNG copies in one portable folder.</p>
<section><h2>1. Listen and compare</h2><p><a href="CERBERUS-5200.html">Compare Cerberus at 5,200 RPM</a> at full and light load: source, BESS 0.8.4, and BESS 0.8.3. <a href="listening/index.html">Compare all twelve vehicles</a>: A is the source bank played through BESS, B is the 0.8.4 render, and C is the previous render shown on the page. All WAVs are mono 48 kHz / 24-bit.</p></section>
<section><h2>2. Adjust settings in BESS</h2><p>Run <a href="BESS.exe">BESS.exe</a>, then open a project in <strong>listening</strong> or import a ZIP from <strong>sources</strong>. Live playback prefers 48 kHz when the output device supports it; the actual format is shown in the application.</p></section>
<section><h2>3. Try the BeamNG copies</h2><p>Disable the original vehicle before enabling its BESS copy. The ZIPs include mono 48 kHz / 24-bit loops. Driving sound and interior/exterior views still need an in-game listening test.</p><ul>{''.join(mods)}</ul></section>
<section><h2>4. Results and limitations</h2><p><a href="documentation/reports/CERBERUS-5200-0.8.4-2026-09-22.md">0.8.4 measurements and limitations</a> · <a href="documentation/FINAL-TESTS.md">Test checklist</a> · <a href="beamng/verification.json">BeamNG copy audit</a></p></section></html>'''
(out / "START.html").write_text(page, encoding="utf-8")

hashes = {}
for path in sorted(out.rglob("*")):
    if path.is_file():
        with path.open("rb") as stream:
            hashes[path.relative_to(out).as_posix()] = hashlib.file_digest(stream, "sha256").hexdigest()
(out / "SHA256.json").write_text(json.dumps(hashes, indent=2), encoding="utf-8")
print(f"{out}: {len(hashes)} files, {len(projects)} portable projects")
