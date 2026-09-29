"""Measure `final_proof` WAVs; with two dirs (before after), print deltas.

Usage: python3 tools/proof_compare.py [before_dir] after_dir
"""
import sys, glob, os, wave
import numpy as np
BANDS = [(0, 250), (250, 1000), (1000, 4000), (4000, 8000), (8000, 24000)]
def read(p):
    w = wave.open(p); b = w.readframes(w.getnframes()); sw = w.getsampwidth(); r = w.getframerate()
    if sw == 3:
        a = np.frombuffer(b, np.uint8).reshape(-1, 3).astype(np.int32)
        x = a[:, 0] | (a[:, 1] << 8) | (a[:, 2] << 16); x = np.where(x >= 1 << 23, x - (1 << 24), x) / 2**23
    elif sw == 2: x = np.frombuffer(b, np.int16) / 32768
    else: x = np.frombuffer(b, np.int32) / 2**31
    return x, r
def measure(p):
    x, r = read(p)
    X = np.abs(np.fft.rfft(x)) ** 2; f = np.fft.rfftfreq(len(x), 1 / r); tot = X.sum() + 1e-30
    return dict(rms=10 * np.log10(np.mean(x**2) + 1e-20), peak=20 * np.log10(np.abs(x).max() + 1e-12),
                centroid=(f * X).sum() / tot, clip=int((np.abs(x) > 0.89).sum()),
                bands=[100 * X[(f >= lo) & (f < hi)].sum() / tot for lo, hi in BANDS])
def row(name, m): return f"| {name} | {m['rms']:.1f} | {m['peak']:.1f} | {m['centroid']:.0f} | " + " / ".join(f"{b:.0f}" for b in m['bands']) + f" | {m['clip']} |"
dirs = sys.argv[1:]
print("| clip | RMS dBFS | peak dBFS | centroid Hz | bands % <250/1k/4k/8k/>8k | clipped |\n|---|---|---|---|---|---|")
for p in sorted(glob.glob(os.path.join(dirs[-1], "*.wav"))):
    n = os.path.basename(p)[:-4]; m = measure(p)
    if len(dirs) == 2 and os.path.exists(os.path.join(dirs[0], n + ".wav")):
        b = measure(os.path.join(dirs[0], n + ".wav"))
        print(f"| {n} | {b['rms']:.1f} → {m['rms']:.1f} | {b['peak']:.1f} → {m['peak']:.1f} | {b['centroid']:.0f} → {m['centroid']:.0f} | "
              + " / ".join(f"{x:.0f}→{y:.0f}" for x, y in zip(b['bands'], m['bands'])) + f" | {b['clip']} → {m['clip']} |")
    else: print(row(n, m))
