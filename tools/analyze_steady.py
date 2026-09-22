"""Measure level modulation in fixed-RPM 48 kHz / PCM24 BESS renders."""
import json
import math
import statistics
import sys
import wave
from pathlib import Path


def levels(path: Path, window_ms: int) -> list[float]:
    with wave.open(str(path), "rb") as wav:
        assert (wav.getframerate(), wav.getnchannels(), wav.getsampwidth()) == (48000, 1, 3)
        raw = wav.readframes(wav.getnframes())
    width = 48 * window_ms
    first = 48000  # Ignore gain/energy-follower startup.
    squares = []
    for i in range(0, len(raw), 3):
        value = int.from_bytes(raw[i : i + 3], "little", signed=True) / 8388608.0
        squares.append(value * value)
    energy = [0.0]
    for square in squares:
        energy.append(energy[-1] + square)
    return [
        math.sqrt((energy[end] - energy[start]) / width)
        for start in range(first, len(squares) - width + 1, width)
        for end in [start + width]
    ]


def metrics(path: Path, window_ms: int) -> dict:
    values = levels(path, window_ms)
    ordered = sorted(values)
    mean = statistics.mean(values)
    return {
        "mean": mean,
        "cv": statistics.pstdev(values) / mean,
        "min": ordered[0],
        "p05": ordered[int(0.05 * (len(ordered) - 1))],
        "p95": ordered[int(0.95 * (len(ordered) - 1))],
        "max": ordered[-1],
        "max_adjacent_db": max(
            abs(20 * math.log10(a / b)) for a, b in zip(values, values[1:])
        ),
    }


if __name__ == "__main__":
    results = {}
    for name in sys.argv[1:]:
        folder = Path(name)
        results[str(folder)] = {
            label: {
                f"{window_ms}ms": metrics(folder / f"{label}.wav", window_ms)
                for window_ms in (20, 50, 100)
            }
            for label in ("source", "bess")
        }
    print(json.dumps(results, indent=2))
