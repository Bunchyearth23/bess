"""Audit the Cerberus correction and unchanged A references in BESS 0.8.4."""
import hashlib
import json
import wave
from pathlib import Path

from analyze_steady import metrics

root = Path(__file__).resolve().parents[1]
before = root / "output/corpus-0.8.3-pcm24"
after = root / "output/corpus-0.8.4-en"
previous = {
    row["archive"]: row
    for row in json.loads((before / "corpus.json").read_text(encoding="utf-8"))["vehicles"]
}
current = json.loads((after / "corpus.json").read_text(encoding="utf-8"))["vehicles"]
assert len(previous) == len(current) == 12


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check_wav(path: Path) -> None:
    with wave.open(str(path), "rb") as wav:
        assert (wav.getframerate(), wav.getnchannels(), wav.getsampwidth()) == (48000, 1, 3)


tracks = []
for row in current:
    assert row["status"] == "ok"
    prior = previous[row["archive"]]
    assert row["source"]["fingerprint"] == prior["source"]["fingerprint"]
    source_old = before / prior["folder"] / "01-source-automation.wav"
    source_new = after / row["folder"] / "01-source-automation.wav"
    older = list((before / prior["folder"]).glob("02-*.wav"))
    assert len(older) == 1
    bess_old = older[0]
    bess_new = after / row["folder"] / "02-bess-enhanced.wav"
    for path in (source_old, source_new, bess_old, bess_new):
        check_wav(path)
    assert digest(source_old) == digest(source_new), row["archive"]
    tracks.append({
        "archive": row["archive"],
        "source_sha256": digest(source_new),
        "bess_083_sha256": digest(bess_old),
        "bess_084_sha256": digest(bess_new),
        "bess_changed": digest(bess_old) != digest(bess_new),
    })

steady = {}
for load in ("full", "light"):
    old = root / f"output/steady-0.8.3-cerberus-5200-{load}"
    new = root / f"output/steady-0.8.4-cerberus-5200-en-{load}"
    assert digest(old / "source.wav") == digest(new / "source.wav")
    steady[load] = {
        "source": metrics(new / "source.wav", 50),
        "bess_083": metrics(old / "bess.wav", 50),
        "bess_084": metrics(new / "bess.wav", 50),
    }
assert steady["full"]["bess_084"]["cv"] < steady["full"]["bess_083"]["cv"] * 0.75
assert steady["light"]["bess_084"]["cv"] < steady["light"]["bess_083"]["cv"] * 0.65

result = {
    "method": "50 ms RMS windows from second 1 to 8 at a fixed 5200 rpm, 48 kHz mono PCM24; CV = standard deviation divided by mean.",
    "comparison": tracks,
    "unchanged_source_wavs": len(tracks),
    "changed_bess_wavs": sum(row["bess_changed"] for row in tracks),
    "steady_5200": steady,
}
path = root / "docs/reports/CERBERUS-5200-0.8.4-metrics.json"
path.write_text(json.dumps(result, indent=2), encoding="utf-8")
print(f"{len(tracks)} identical A references; {result['changed_bess_wavs']} changed B renders")
for load, values in steady.items():
    print(f"{load}: 50 ms CV B {values['bess_083']['cv']:.3f} -> {values['bess_084']['cv']:.3f}")
