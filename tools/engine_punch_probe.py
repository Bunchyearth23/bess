"""Inspect steady engine-cycle repetition and short-time level movement."""

from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np

from r8_audio_metrics import read_wav, spectrum_power


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--rpm", type=float, required=True)
    parser.add_argument("--step", action="store_true")
    parser.add_argument("files", nargs="+", type=Path)
    args = parser.parse_args()
    for path in args.files:
        rate, samples = read_wav(path)
        if args.step:
            def window_rms(start: float, stop: float) -> float:
                part = samples[round(start * rate):round(stop * rate)]
                return float(np.sqrt(np.mean(part * part)))

            low = window_rms(1.25, 1.75)
            attack = window_rms(2.15, 2.35)
            high = window_rms(3.25, 3.75)
            print(
                f"{path.name}: low={low:.6f} attack={attack:.6f} high={high:.6f} "
                f"attack_over_high_db={20*np.log10(attack/high):.3f} "
                f"peak={np.max(np.abs(samples)):.4f}"
            )
            continue
        period = 120.0 / args.rpm
        end = len(samples) / rate - period
        cycles = []
        for start in np.arange(1.25, end, period):
            positions = (start + np.arange(512) * period / 512) * rate
            cycle = np.interp(positions, np.arange(len(samples)), samples)
            cycle -= cycle.mean()
            cycles.append(cycle)
        levels = np.array([np.sqrt(np.mean(cycle * cycle)) for cycle in cycles])
        similarity = [
            float(np.dot(a, b) / np.sqrt(np.dot(a, a) * np.dot(b, b)))
            for a, b in zip(cycles, cycles[1:])
        ]
        frame = round(rate * 0.05)
        frames = samples[rate : len(samples) - (len(samples) - rate) % frame].reshape(-1, frame)
        window_levels = np.sqrt(np.mean(frames * frames, axis=1))
        span = 20 * np.log10(np.percentile(window_levels, 95) / np.percentile(window_levels, 5))
        frequencies, power = spectrum_power(samples[rate:], rate)
        basis = power[(frequencies >= 20) & (frequencies < 2000)].sum()
        bands = [
            power[(frequencies >= low) & (frequencies < high)].sum() / basis
            for low, high in [(20, 80), (80, 150), (150, 300), (300, 500), (500, 2000)]
        ]
        dominant = np.argmax(power[(frequencies >= 20) & (frequencies < 500)])
        dominant_hz = frequencies[(frequencies >= 20) & (frequencies < 500)][dominant]
        print(
            f"{path.name}: cycles={len(cycles)} "
            f"cycle_similarity={np.median(similarity):.4f} "
            f"cycle_level_cv={levels.std()/levels.mean():.4f} "
            f"50ms_span_db={span:.3f} "
            f"20-80/80-150/150-300/300-500/500-2k={[round(x, 3) for x in bands]} "
            f"dominant_hz={dominant_hz:.0f}"
        )


if __name__ == "__main__":
    main()
