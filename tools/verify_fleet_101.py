"""Check private local BESS 0.10.1 BeamNG exports against their Automation ZIPs.

The source and output directories are arguments. This script never bundles or
publishes either set of vehicle archives.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
from pathlib import Path
import wave
import zipfile


def sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def check(source_dir: Path, output_dir: Path) -> dict:
    sources = sorted(source_dir.glob("bunchyearth23_*.zip"))
    assert len(sources) == 12, f"Expected twelve Automation ZIPs, found {len(sources)}"
    report: dict = {"version": "0.10.1", "vehicles": [], "total_wavs": 0, "total_entries": 0}
    all_new_paths: set[str] = set()
    for source in sources:
        slug = source.stem.removeprefix("bunchyearth23_")
        folder = output_dir / slug
        manifest = json.loads((folder / "manifest.json").read_text(encoding="utf-8"))
        project = json.loads((folder / "settings.bess.json").read_text(encoding="utf-8"))
        variant = folder / manifest["zip_file"]
        assert variant.is_file() and len(list(folder.glob("bess-variant-*.zip"))) == 1, slug
        assert manifest["version"] == "0.10.1", slug
        assert project["hybrid"]["procedural"] is False, slug
        assert manifest["settings"]["procedural"] is False, slug
        assert manifest["source_sha256"] == sha256(source), slug
        assert manifest["display_name"].endswith("(BESS)"), slug
        assert manifest["config_path"].startswith(f"vehicles/{slug}/"), slug
        with zipfile.ZipFile(source) as original, zipfile.ZipFile(variant) as addon:
            old_paths = set(original.namelist())
            paths = addon.namelist()
            assert len(paths) == len(set(paths)), slug
            assert not old_paths.intersection(paths), f"Overlapping original paths: {slug}"
            assert not all_new_paths.intersection(paths), f"Overlapping add-on paths: {slug}"
            all_new_paths.update(paths)
            assert addon.testzip() is None, f"Archive CRC failed: {slug}"
            wavs = [p for p in paths if p.lower().endswith(".wav")]
            assert set(wavs) == set(manifest["wav_paths"] + manifest["engine_wav_paths"]), slug
            assert len(wavs) > 0 and len(manifest["wav_paths"]) == len(manifest["engine_wav_paths"]), slug
            for name in wavs:
                with addon.open(name) as stream:
                    header = stream.read(128)
                with wave.open(io.BytesIO(header), "rb") as audio:
                    assert (audio.getnchannels(), audio.getframerate(), audio.getsampwidth()) == (1, 48_000, 3), name
                    assert audio.getnframes() > 0, name
            report["total_wavs"] += len(wavs)
            report["total_entries"] += len(paths)
            report["vehicles"].append({
                "slug": slug,
                "source_sha256": manifest["source_sha256"],
                "variant": variant.name,
                "variant_sha256": sha256(variant),
                "entries": len(paths),
                "wavs": len(wavs),
            })
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source_dir", type=Path)
    parser.add_argument("output_dir", type=Path)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    result = check(args.source_dir, args.output_dir)
    if args.report:
        args.report.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(f"PASS: {len(result['vehicles'])} original/add-on pairs, {result['total_entries']} disjoint entries, {result['total_wavs']} mono PCM24 WAVs")
