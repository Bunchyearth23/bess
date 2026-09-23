"""Render and measure the interface-only experimental mode on local Automation ZIPs.

Run after `cargo build --release`:
  python tools/experimental_fleet_audit.py cars output/experimental-fleet-audit

The output contains level-matched listening WAVs and objective diagnostics.
Band shares and short-term modulation are not measures of perceived realism.
Vehicle ZIPs and generated WAVs remain outside the public repository.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import wave
from pathlib import Path

from r8_audio_metrics import metrics


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cars", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--executable", type=Path, default=Path("target/release/bess.exe"))
    parser.add_argument("--rpm", type=int, default=3000)
    parser.add_argument("--cerberus-rpm", type=int, default=5200)
    parser.add_argument("--load", type=float, default=0.7)
    args = parser.parse_args()
    if args.rpm < 200 or args.cerberus_rpm < 200 or not 0 <= args.load <= 1:
        parser.error("RPM must be at least 200 and load must be between 0 and 1")
    archives = sorted(args.cars.glob("*.zip"))
    if not archives:
        parser.error("No Automation ZIP files found")
    executable = args.executable.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    rows = []
    for index, archive in enumerate(archives, 1):
        folder = args.output / archive.stem
        rpm = args.cerberus_rpm if "cerberus_a" in archive.stem else args.rpm
        print(f"{index}/{len(archives)} {archive.stem}: {rpm} rpm, {args.load} load", flush=True)
        subprocess.run(
            [str(executable), "--steady-procedural", str(archive.resolve()),
             str(folder.resolve()), str(rpm), str(args.load)],
            check=True,
        )
        for name in ("01-source-automation.wav", "02-source-guided.wav", "03-generated.wav"):
            with wave.open(str(folder / name), "rb") as wav:
                if (wav.getnchannels(), wav.getframerate(), wav.getsampwidth()) != (1, 48000, 3):
                    raise ValueError(f"Unexpected WAV format: {folder / name}")
        source = metrics(folder / "01-source-automation.wav")[2]
        guided = metrics(folder / "02-source-guided.wav")[2]
        generated = metrics(folder / "03-generated.wav")[2]
        if not 0 < generated["absolute_peak"] < 0.95:
            raise ValueError(f"Silent or clipped generated sound: {archive.name}")
        rows.append({
            "vehicle": archive.stem,
            "requested_rpm": rpm,
            "load": args.load,
            "folder": str(folder.resolve()),
            "source": source,
            "source_guided": guided,
            "experimental": generated,
            "generated_minus_source_band_share": {
                band: round(generated["band_power_share_of_80_10000_hz"][band]
                            - share, 5)
                for band, share in source["band_power_share_of_80_10000_hz"].items()
            },
        })
        (args.output / "fleet.json").write_text(
            json.dumps({"vehicles": rows}, indent=2) + "\n", encoding="utf-8"
        )
    print(f"Completed {len(rows)} vehicles: {args.output / 'fleet.json'}")


if __name__ == "__main__":
    main()
