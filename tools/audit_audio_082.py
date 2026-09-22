"""Compare the 0.8.2 real-archive corpus with its unchanged 0.8.1 source."""
from pathlib import Path
import json

from compare_audio_pairs import measure

root = Path(__file__).resolve().parents[1]
old = root / "output/corpus-0.8.0"
new = root / "output/corpus-0.8.2"
previous = {row["archive"]: row for row in json.loads((old / "corpus.json").read_text(encoding="utf-8"))["vehicles"]}
current = json.loads((new / "corpus.json").read_text(encoding="utf-8"))["vehicles"]
assert len(current) == len(previous) == 12
rows = []
for row in current:
    assert row["status"] == "ok"
    prior = previous[row["archive"]]
    assert row["source"]["fingerprint"] == prior["source"]["fingerprint"]
    old_dir = old / prior["folder"]
    new_dir = new / row["folder"]
    assert (new_dir / "01-source-automation.wav").read_bytes() == (old_dir / "01-source-automation.wav").read_bytes()
    new_enhanced = sorted(new_dir.glob("02-*.wav"))
    old_enhanced = sorted(old_dir.glob("02-*.wav"))
    assert len(new_enhanced) == len(old_enhanced) == 1
    assert new_enhanced[0].read_bytes() != old_enhanced[0].read_bytes()
    assert row["metadata"]["automation_engine"] is not None
    m = measure(new_dir)
    assert m["peak_bess"] < 0.95
    assert abs(20 * __import__("math").log10(m["rms_bess"] / m["rms_source"])) < 0.001
    rows.append({
        "archive": row["archive"],
        "engine": row["metadata"]["automation_engine"],
        "rms_difference": m["difference_rms"],
        "correlation": m["correlation"],
        "peak_bess": m["peak_bess"],
        "centroid_source_hz": m["spectrum_source"]["centroid_hz"],
        "centroid_bess_hz": m["spectrum_bess"]["centroid_hz"],
        "high_over_low_source_db": m["spectrum_source"]["high_over_low_db"],
        "high_over_low_bess_db": m["spectrum_bess"]["high_over_low_db"],
    })
result = {
    "method": "Identical 16 s cycle; exact source file equality; B at matched integrated RMS; spectral energy over 4096-sample windows. Metrics do not establish subjective realism.",
    "source_unchanged": len(rows),
    "new_bess_renders": len(rows),
    "engine_metadata_matches": len(rows),
    "vehicles": rows,
}
path = root / "docs/reports/natural-0.8.2-audio-metrics.json"
path.write_text(json.dumps(result, indent=2), encoding="utf-8")
print(f"{path}: {len(rows)} verified vehicles")
