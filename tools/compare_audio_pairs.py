"""Objective diagnostics for two level-matched BESS audition WAV files."""
import json
import sys
import wave

import numpy as np


def read(path):
    with wave.open(path) as wav:
        assert (wav.getframerate(), wav.getnchannels(), wav.getsampwidth()) == (48000, 1, 3)
        raw = np.frombuffer(wav.readframes(wav.getnframes()), np.uint8).reshape(-1, 3)
    values = raw[:, 0].astype(np.int32) | (raw[:, 1].astype(np.int32) << 8) | (raw[:, 2].astype(np.int32) << 16)
    return np.where(values >= 8388608, values - 16777216, values).astype(np.float64) / 8388608.0


def measure(folder):
    a = read(str(folder / "01-source-automation.wav"))
    enhanced = sorted(folder.glob("02-*.wav"))
    assert len(enhanced) == 1, f"Expected one enhanced WAV in {folder}"
    b = read(str(enhanced[0]))
    assert len(a) == len(b) == 16 * 48000
    dot = np.dot(a, b)
    energy_a, energy_b = np.dot(a, a), np.dot(b, b)
    blocks_a = np.sqrt(np.mean(a.reshape(16, 48000) ** 2, axis=1))
    blocks_b = np.sqrt(np.mean(b.reshape(16, 48000) ** 2, axis=1))
    def spectrum(signal):
        frames = signal[:len(signal) // 4096 * 4096].reshape(-1, 4096)
        power = np.mean(np.abs(np.fft.rfft(frames * np.hanning(4096), axis=1)) ** 2, axis=0)
        hz = np.fft.rfftfreq(4096, 1 / 48000)
        return {
            "centroid_hz": float(np.sum(hz * power) / np.sum(power)),
            "high_over_low_db": float(10 * np.log10(np.sum(power[hz >= 2000]) / max(np.sum(power[(hz >= 80) & (hz < 2000)]), 1e-20))),
        }
    return {
        "rms_source": float(np.sqrt(energy_a / len(a))),
        "rms_bess": float(np.sqrt(energy_b / len(b))),
        "peak_source": float(np.max(np.abs(a))),
        "peak_bess": float(np.max(np.abs(b))),
        "difference_rms": float(np.sqrt(np.mean((a - b) ** 2))),
        "correlation": float(dot / np.sqrt(energy_a * energy_b)),
        "spectrum_source": spectrum(a),
        "spectrum_bess": spectrum(b),
        "segment_bess_to_source_db": [float(20 * np.log10(max(y, 1e-12) / max(x, 1e-12))) for x, y in zip(blocks_a, blocks_b)],
    }


if __name__ == "__main__":
    from pathlib import Path

    result = {Path(name).name: measure(Path(name)) for name in sys.argv[1:]}
    print(json.dumps(result, indent=2))
