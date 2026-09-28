"""Compare physical_idle_probe outputs; create attenuated-only listening copies.

Requires NumPy. Usage: python physical_idle_analysis.py BEFORE_DIR AFTER_DIR
Original WAVs remain untouched. Measurements use seconds 2–8 throughout.
"""
import csv
import html
import json
import struct
import sys
import wave
from pathlib import Path

import numpy as np


def read_pcm24(path):
    data = path.read_bytes()
    offset = 12
    assert data[:4] == b"RIFF" and data[8:12] == b"WAVE", path
    rate = None
    payload = None
    while offset + 8 <= len(data):
        tag = data[offset:offset + 4]
        size = struct.unpack_from("<I", data, offset + 4)[0]
        chunk = data[offset + 8:offset + 8 + size]
        if tag == b"fmt ":
            _, channels, rate, _, _, bits = struct.unpack_from("<HHIIHH", chunk)
            assert channels == 1 and bits == 24, path
        elif tag == b"data":
            payload = chunk
        offset += 8 + size + size % 2
    assert rate is not None and payload is not None, path
    values = np.frombuffer(payload, dtype=np.uint8).reshape(-1, 3).astype(np.int32)
    values = values[:, 0] + (values[:, 1] << 8) + (values[:, 2] << 16)
    values = np.where(values >= 8388608, values - 16777216, values) / 8388608
    assert np.isfinite(values).all()
    return values, rate


def write_pcm24(path, samples, rate):
    assert np.isfinite(samples).all() and np.max(np.abs(samples)) <= 1
    values = np.rint(samples * 8388608).astype(np.int32)
    assert np.max(values) <= 8388607 and np.min(values) >= -8388608
    raw = np.column_stack((values & 255, (values >> 8) & 255, (values >> 16) & 255))
    with wave.open(str(path), "wb") as writer:
        writer.setparams((1, 3, rate, len(samples), "NONE", "not compressed"))
        writer.writeframes(raw.astype(np.uint8).tobytes())


def rms(samples):
    return float(np.sqrt(np.mean(samples * samples)))


def measure(path):
    values, rate = read_pcm24(path)
    x = values[2 * rate:8 * rate]
    assert len(x) == rate * 6
    power = np.abs(np.fft.rfft((x - x.mean()) * np.hanning(len(x)))) ** 2
    frequencies = np.fft.rfftfreq(len(x), 1 / rate)
    peak_index = np.argmax(np.where(frequencies > 5, power, 0))
    band = lambda a, b: float(100 * power[(frequencies >= a) & (frequencies < b)].sum() / power.sum())
    envelope = np.sqrt(np.mean(x.reshape(-1, rate // 100) ** 2, axis=1))
    return {
        "rms_dbfs": 20 * np.log10(rms(x)),
        "peak_dbfs": float(20 * np.log10(np.max(np.abs(x)))),
        "dominant_frequency_hz": float(frequencies[peak_index]),
        "power_below250_pct": band(0, 250),
        "power500_2000_pct": band(500, 2000),
        "envelope_p90_p10_db": float(20 * np.log10(np.percentile(envelope, 90) / np.percentile(envelope, 10))),
        "envelope_cv": float(envelope.std() / envelope.mean()),
    }


def main():
    before, after = map(Path, sys.argv[1:3])
    rows = []
    for name, slug in [("Inline-4", "i4"), ("V8 cross-plane", "v8-cross-plane"), ("V12 60°", "v12-60")]:
        for variant, directory in [("before", before), ("after", after)]:
            states = np.genfromtxt(directory / f"{slug}-state.csv", delimiter=",", names=True)[2000:]
            for stem in ["exhaust", "orbit"]:
                row = {"engine": name, "variant": variant, "stem": stem}
                row.update(measure(directory / f"{slug}-{stem}.wav"))
                row.update({
                    "misfires_delta": int(states["misfires"][-1] - states["misfires"][0]),
                    "combustion_heat_j": float(states["heat_j"].sum()),
                    "rpm_min": float(states["rpm"].min()),
                    "rpm_max": float(states["rpm"].max()),
                })
                rows.append(row)
    original_before, rate_before = read_pcm24(before / "i4-orbit.wav")
    original_after, rate_after = read_pcm24(after / "i4-orbit.wav")
    assert rate_before == rate_after == 48000
    crops = [original_before[96000:384000], original_after[96000:384000]]
    target = min(map(rms, crops))
    copies = []
    for label, samples in zip(["before", "after"], crops):
        gain = min(1., target / rms(samples))
        filename = f"i4-{label}-matched.wav"
        write_pcm24(after / filename, samples * gain, rate_after)
        restored, _ = read_pcm24(after / filename)
        copies.append({"variant": label, "file": filename, "gain": gain,
                       "gain_db": float(20 * np.log10(gain)),
                       "rms_dbfs_after_pcm24": float(20 * np.log10(rms(restored)))})
    report = {"window_s": [2, 8], "spectrum": "Hann-windowed, DC removed; power ratios",
              "envelope": "RMS per non-overlapping 10 ms block; not a realism score",
              "levels": "Unwindowed RMS/peak of source samples",
              "rows": rows, "listening_copies": copies,
              "copy_policy": "Crop only, then constant attenuation of the louder clip; no boost, AGC, equalizer or added fades. Original WAVs unchanged."}
    (after / "comparison.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    with (after / "comparison.csv").open("w", encoding="utf-8", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)
    players = "".join(f'<article><h2>{"Avant" if c["variant"] == "before" else "Après"}</h2><audio controls preload="metadata" src="{c["file"]}"></audio><p>Atténuation constante : {c["gain_db"]:.2f} dB · RMS : {c["rms_dbfs_after_pcm24"]:.2f} dBFS.</p><a href="{c["file"]}" download>Télécharger cette copie</a></article>' for c in copies)
    table = "".join(f'<tr><td>{html.escape(r["engine"])}</td><td>{"Avant" if r["variant"] == "before" else "Après"}</td><td>{r["dominant_frequency_hz"]:.2f}</td><td>{r["power_below250_pct"]:.2f} %</td><td>{r["rms_dbfs"]:.2f}</td></tr>' for r in rows if r["stem"] == "orbit")
    page = f'''<!doctype html><html lang="fr"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>BESS — comparaison du ralenti</title><style>body{{margin:0;background:#111823;color:#edf2f8;font:17px/1.6 system-ui}}main{{max-width:900px;margin:auto;padding:32px 24px}}article{{border:1px solid #45536a;padding:22px;border-radius:12px;margin:18px 0}}audio{{width:100%}}a{{color:#a7d3ff}}table{{width:100%;border-collapse:collapse}}td,th{{padding:9px;text-align:left;border-bottom:1px solid #45536a}}</style><main><h1>Ralenti I4 : avant / après</h1><p>Régime imposé de 850 tr/min, papillon 5 %, caméra Orbit. Copies de six secondes extraites entre 2 et 8 s des rendus originaux, PCM 24 bits / 48 kHz.</p><p><strong>Comparaison à RMS commun :</strong> seul le fichier le plus fort est atténué, avec un gain constant. Aucun AGC n’est appliqué au moteur. Les originaux restent inchangés. Les chiffres du tableau décrivent les originaux, avant cette atténuation.</p>{players}<h2>Mesures des rendus Orbit originaux</h2><table><thead><tr><th>Moteur</th><th>Version</th><th>Pic spectral (Hz)</th><th>Énergie sous 250 Hz</th><th>RMS (dBFS)</th></tr></thead><tbody>{table}</tbody></table><p><a href="comparison.json">Méthode et mesures JSON</a> · <a href="comparison.csv">Mesures CSV</a></p><p>La mesure d’enveloppe décrit la pulsation du signal ; elle n’est pas un score de réalisme. Aucun jugement à l’oreille n’est supposé par ces résultats.</p></main></html>'''
    (after / "index.html").write_text(page, encoding="utf-8")
    for row in rows:
        if row["stem"] == "orbit":
            print(json.dumps(row, ensure_ascii=False))
    print("Listening copies:", json.dumps(copies))


if __name__ == "__main__":
    main()
