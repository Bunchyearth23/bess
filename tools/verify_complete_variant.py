"""Verify a complete original-plus-BESS export, optionally against a legacy add-on.

This reads existing artifacts only. The JSON receipt must not already exist.
"""
import argparse
import copy
import hashlib
import io
import json
from pathlib import Path
import wave
import zipfile


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def members(archive: zipfile.ZipFile) -> dict[str, str]:
    names = archive.namelist()
    assert len(names) == len(set(names)), "Duplicate ZIP member"
    assert archive.testzip() is None, "ZIP CRC failure"
    return {name: digest(archive.read(name)) for name in names}


def sound_points(archive: zipfile.ZipFile, manifest: dict) -> dict[tuple, bytes]:
    points = {}
    for stem, key in (("exhaust", "blend_path"), ("engine", "engine_blend_path")):
        blend = json.loads(archive.read(manifest[key]))
        for load, row in enumerate(blend["samples"]):
            for path, rpm, *_ in row:
                assert (stem, load, rpm) not in points
                payload = archive.read(path)
                with wave.open(io.BytesIO(payload)) as wav:
                    assert (wav.getnchannels(), wav.getsampwidth(), wav.getframerate()) == (1, 3, 48000)
                    # Whole-cycle f32 rounding can undershoot two seconds by
                    # one frame; the exporter checks each knot's exact length.
                    assert wav.getnframes() >= 95999
                points[stem, load, rpm] = payload
    return points


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--export-dir", required=True, type=Path)
    parser.add_argument("--legacy-dir", type=Path)
    parser.add_argument("--report", required=True, type=Path)
    args = parser.parse_args()
    manifest = json.loads((args.export_dir / "manifest.json").read_text(encoding="utf-8"))
    name = manifest["zip_file"]
    assert Path(name).name == name and name.endswith(".zip")
    candidate = args.export_dir / name
    source_sha = digest(args.source.read_bytes())
    with zipfile.ZipFile(args.source) as source, zipfile.ZipFile(candidate) as exported:
        original = members(source)
        current = members(exported)
        assert original.keys() <= current.keys(), "Original member removed"
        assert all(current[name] == value for name, value in original.items()), "Original member changed"
        marker = json.loads(exported.read("bess-export.json"))
        assert marker["version"] == 1 and marker["kind"] == "bess-variant-vehicle"
        assert marker["source_archive_sha256"] == source_sha
        additions = {item["path"]: item["sha256"] for item in marker["added_files"]}
        assert len(additions) == len(marker["added_files"])
        assert additions.keys().isdisjoint(original)
        assert set(current) - set(original) == set(additions) | {"bess-export.json"}
        assert all(current[name] == value for name, value in additions.items())
        config_path = marker["configuration_path"]
        assert config_path == manifest["config_path"]
        assert config_path in additions and config_path.endswith(".pc")
        original_config = json.loads(source.read(manifest["source_config"]))
        variant_config = json.loads(exported.read(config_path))
        expected = copy.deepcopy(original_config)
        expected["parts"]["Camso_Engine"] = manifest["engine_part"]
        assert variant_config == expected, "Variant changed non-engine configuration fields"
        assert original_config["parts"]["Camso_Engine"] != manifest["engine_part"]
        info = json.loads(exported.read(manifest["info_path"]))
        assert info["Configuration"] == manifest["display_name"]
        assert "BESS" in info["Configuration"]
        engine = exported.read(manifest["engine_path"]).decode("utf-8")
        for identifier in (manifest["engine_part"], manifest["sample_id"], manifest["engine_sample_id"]):
            assert identifier in engine, f"Engine routing missing {identifier}"
        points = sound_points(exported, manifest)
        wav_paths = manifest["wav_paths"] + manifest["engine_wav_paths"]
        assert len(wav_paths) == len(points) and set(wav_paths) <= additions.keys()
        assert set(wav_paths).isdisjoint(original)
        legacy_match = None
        if args.legacy_dir:
            legacy_manifest = json.loads((args.legacy_dir / "manifest.json").read_text(encoding="utf-8"))
            with zipfile.ZipFile(args.legacy_dir / legacy_manifest["zip_file"]) as legacy:
                legacy_points = sound_points(legacy, legacy_manifest)
            assert points.keys() == legacy_points.keys()
            differences = [key for key in points if points[key] != legacy_points[key]]
            assert not differences, f"Audio differs from legacy variant: {differences}"
            legacy_match = len(points)
        result = {
            "source": str(args.source.resolve()), "source_sha256": source_sha,
            "candidate": str(candidate.resolve()), "candidate_sha256": digest(candidate.read_bytes()),
            "unchanged_original_members": len(original), "added_members": len(additions),
            "original_configurations": sum(name.endswith(".pc") for name in original),
            "exported_configurations": sum(name.endswith(".pc") for name in current),
            "variant_id": marker["variant_id"], "display_name": info["Configuration"],
            "generated_wavs": len(points), "legacy_byte_identical_wavs": legacy_match,
            "passed": True,
        }
    with args.report.open("x", encoding="utf-8") as receipt:
        json.dump(result, receipt, indent=2)
    print(json.dumps(result))


if __name__ == "__main__":
    main()
