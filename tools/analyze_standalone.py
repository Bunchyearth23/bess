"""Small, dependency-free PCM24 level and steady-window spectrum audit."""
import json
import math
import sys
import wave
from pathlib import Path


def fft(values):
    n = len(values)
    j = 0
    for i in range(1, n):
        bit = n >> 1
        while j & bit:
            j ^= bit
            bit >>= 1
        j ^= bit
        if i < j:
            values[i], values[j] = values[j], values[i]
    length = 2
    while length <= n:
        root = complex(math.cos(-2 * math.pi / length), math.sin(-2 * math.pi / length))
        for start in range(0, n, length):
            w = 1 + 0j
            for offset in range(length // 2):
                even = values[start + offset]
                odd = values[start + offset + length // 2] * w
                values[start + offset] = even + odd
                values[start + offset + length // 2] = even - odd
                w *= root
        length <<= 1


def analyze(path):
    with wave.open(str(path), "rb") as wav:
        if (wav.getnchannels(), wav.getsampwidth()) != (1, 3):
            raise ValueError("expected mono PCM24 WAV")
        rate = wav.getframerate()
        raw = wav.readframes(wav.getnframes())
    samples = [int.from_bytes(raw[i:i + 3], "little", signed=True) / 8388608
               for i in range(0, len(raw), 3)]
    mean = sum(samples) / len(samples)
    ac_rms = math.sqrt(sum((s - mean) ** 2 for s in samples) / len(samples))
    n = 8192
    window = samples[-n:]
    if len(window) < n:
        raise ValueError("WAV is shorter than the analysis window")
    taper = [complex((window[i] - mean) * (0.5 - 0.5 * math.cos(2 * math.pi * i / (n - 1))))
             for i in range(n)]
    fft(taper)
    magnitudes = [abs(v) for v in taper[:n // 2 + 1]]
    local_peaks = [i for i in range(max(1, int(30 * n / rate)), n // 2)
                   if magnitudes[i] >= magnitudes[i - 1] and magnitudes[i] > magnitudes[i + 1]]
    strongest = sorted(local_peaks, key=lambda i: magnitudes[i], reverse=True)[:5]
    return {
        "file": path.name, "rate_hz": rate, "frames": len(samples),
        "ac_rms": round(ac_rms, 7), "mean": round(mean, 7),
        "peak": round(max(map(abs, samples)), 7),
        "samples_at_ceiling": sum(abs(s) >= 0.979 for s in samples),
        "top_spectral_peaks_hz": [round(i * rate / n, 2) for i in strongest],
    }


if __name__ == "__main__":
    results = [analyze(Path(arg)) for arg in sys.argv[1:]]
    print(json.dumps(results, indent=2))
