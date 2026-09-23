"""Verify the 12 x 3 local BeamNG profile exports before installation."""

from __future__ import annotations

import argparse
import io
import json
import pathlib
import re
import wave
import zipfile

import numpy as np


def pcm24(data: bytes) -> np.ndarray:
    octets = np.frombuffer(data, dtype=np.uint8).reshape(-1, 3).astype(np.int32)
    signed = octets[:, 0] | (octets[:, 1] << 8) | (octets[:, 2] << 16)
    signed = (signed ^ 0x800000) - 0x800000
    return signed.astype(np.float32) / 8388608.0


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=pathlib.Path)
    parser.add_argument("cars", type=pathlib.Path)
    args = parser.parse_args()
    source = {item.stem.removeprefix("bunchyearth23_"): item for item in args.cars.glob("*.zip")}
    assert len(source) == 12, f"Expected 12 original vehicles, found {len(source)}"
    global_paths: set[str] = set()
    report: dict[str, object] = {"vehicles": {}, "total_profiles": 0, "total_wavs": 0}
    for vehicle, source_zip in sorted(source.items()):
        originals = set(zipfile.ZipFile(source_zip).namelist())
        profiles: dict[str, object] = {}
        samples: dict[str, np.ndarray] = {}
        selected_wav: str | None = None
        for profile in ("natural", "smooth", "raw"):
            directory = args.root / f"{vehicle}-{profile}"
            archives = list(directory.glob("bess-variant-*.zip"))
            assert len(archives) == 1, (directory, archives)
            manifest = json.loads((directory / "manifest.json").read_text(encoding="utf-8"))
            assert manifest["profile_name"].lower() == profile, (directory, manifest.get("profile_name"))
            project = json.loads((directory / "settings.bess.json").read_text(encoding="utf-8"))
            assert project["profile_name"].lower() == profile, (directory, project.get("profile_name"))
            assert project["hybrid"]["procedural"] is False, directory
            with zipfile.ZipFile(archives[0]) as archive:
                assert archive.testzip() is None, archives[0]
                paths = archive.namelist()
                assert len(paths) == len(set(paths)), archives[0]
                assert not originals.intersection(paths), archives[0]
                assert not global_paths.intersection(paths), archives[0]
                global_paths.update(paths)
                configs = [path for path in paths if path.endswith(".pc")]
                assert len(configs) == 1, archives[0]
                names = [path for path in paths if "/info_bess_" in path and path.endswith(".json")]
                assert len(names) == 1, archives[0]
                display = json.loads(archive.read(names[0]))["Configuration"]
                assert f"(BESS - {profile.title()})" in display, (archives[0], display)
                wavs = [path for path in paths if path.endswith(".wav")]
                assert wavs and len(wavs) % 2 == 0, archives[0]
                if selected_wav is None:
                    exhaust_names = sorted({path.rsplit("/", 1)[-1] for path in wavs if re.fullmatch(r"EXH_\d+\.wav", path.rsplit("/", 1)[-1])})
                    assert exhaust_names, archives[0]
                    selected_wav = exhaust_names[len(exhaust_names) // 2]
                peak = 0.0
                for wav_path in wavs:
                    with wave.open(io.BytesIO(archive.read(wav_path)), "rb") as wav:
                        assert (wav.getnchannels(), wav.getframerate(), wav.getsampwidth()) == (1, 48000, 3), wav_path
                        if wav_path.endswith("/" + selected_wav):
                            samples[profile] = pcm24(wav.readframes(wav.getnframes()))
                        elif wav_path == wavs[0]:
                            audio = pcm24(wav.readframes(wav.getnframes()))
                            peak = float(np.max(np.abs(audio)))
                report["total_wavs"] += len(wavs)
                report["total_profiles"] += 1
                profiles[profile] = {"archive": archives[0].name, "display": display, "files": len(paths), "wavs": len(wavs), "first_wav_peak": round(peak, 5)}
        assert len(samples) == 3, (vehicle, selected_wav)
        minimum = min(map(len, samples.values()))
        for profile in ("smooth", "raw"):
            diff = samples["natural"][:minimum] - samples[profile][:minimum]
            delta = float(np.sqrt(np.mean(diff * diff)))
            assert delta > 0.0001, (vehicle, profile)
            profiles[profile]["middle_knot_rms_difference"] = round(delta, 5)
        profiles["middle_knot"] = selected_wav
        report["vehicles"][vehicle] = profiles
    assert report["total_profiles"] == 36
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
