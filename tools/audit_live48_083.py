"""Confirm that 0.8.3 changes only the live device rate, not 48 kHz/24-bit WAVs."""
from pathlib import Path
import hashlib
import json

root = Path(__file__).resolve().parents[1]
old = root / "output/corpus-0.8.2"
new = root / "output/corpus-0.8.3-pcm24"
previous = {
    row["archive"]: row
    for row in json.loads((old / "corpus.json").read_text(encoding="utf-8"))["vehicles"]
}
current = json.loads((new / "corpus.json").read_text(encoding="utf-8"))["vehicles"]
assert len(previous) == len(current) == 12
rows = []
for row in current:
    assert row["status"] == "ok"
    prior = previous[row["archive"]]
    assert row["source"]["fingerprint"] == prior["source"]["fingerprint"]
    tracks = {}
    for prefix, name in (("01-", "source"), ("02-", "enhanced")):
        before_candidates = sorted((old / prior["folder"]).glob(f"{prefix}*.wav"))
        after_candidates = sorted((new / row["folder"]).glob(f"{prefix}*.wav"))
        assert len(before_candidates) == len(after_candidates) == 1
        before, after = before_candidates[0], after_candidates[0]
        old_hash = hashlib.sha256(before.read_bytes()).hexdigest()
        new_hash = hashlib.sha256(after.read_bytes()).hexdigest()
        assert old_hash == new_hash, (row["archive"], name)
        tracks[name] = new_hash
    rows.append({"archive": row["archive"], "tracks": tracks})

result = {
    "method": "Byte-for-byte SHA256 equality of every 48 kHz/24-bit A/B WAV between 0.8.2 and 0.8.3; input archive fingerprint equality.",
    "vehicles": rows,
    "unchanged_wavs": len(rows) * 2,
}
path = root / "docs/reports/LIVE-48K-0.8.3-WAV-hashes.json"
path.write_text(json.dumps(result, indent=2), encoding="utf-8")
print(f"{len(rows)} vehicles, {result['unchanged_wavs']} unchanged 48 kHz/24-bit WAVs")
