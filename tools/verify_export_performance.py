"""Time a current release export, then verify its output against an archived release.

The timed interval covers only the CLI process. All hashing/comparison happens
outside it. Inputs are read-only, and both the output and receipt must be new.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time
import zipfile

MARKER = "bess-export.json"


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def members(path: Path) -> dict[str, str]:
    result = {}
    with zipfile.ZipFile(path) as archive:
        for entry in archive.infolist():
            if entry.filename in result:
                raise ValueError(f"Duplicate member: {entry.filename}")
            with archive.open(entry) as stream:
                result[entry.filename] = hashlib.file_digest(stream, "sha256").hexdigest()
    return result


def marker(path: Path) -> dict:
    with zipfile.ZipFile(path) as archive:
        return json.loads(archive.read(MARKER))


def compare(reference: Path, candidate: Path) -> dict:
    before, after = members(reference), members(candidate)
    differences = [name for name in sorted(before.keys() | after.keys())
                   if name != MARKER and before.get(name) != after.get(name)]
    wavs = [name for name in before if name.lower().endswith(".wav")]
    old_marker, new_marker = marker(reference), marker(candidate)
    old_marker.pop("exported_at_unix_ms", None)
    new_marker.pop("exported_at_unix_ms", None)
    marker_equal = old_marker == new_marker
    return {
        "reference_zip": str(reference), "candidate_zip": str(candidate),
        "reference_zip_sha256": digest(reference), "candidate_zip_sha256": digest(candidate),
        "wav_count": len(wavs),
        "identical_wavs": sum(before[name] == after.get(name) for name in wavs),
        "unchanged_other_members": sum(name != MARKER and not name.lower().endswith(".wav")
                                        and value == after.get(name) for name, value in before.items()),
        "differences": differences,
        "marker_equal_except_timestamp": marker_equal,
        "passed": bool(wavs) and not differences and marker_equal,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-timing", type=Path, required=True)
    parser.add_argument("--baseline-zip", type=Path)
    parser.add_argument("--new-exe", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    baseline = json.loads(args.baseline_timing.read_text(encoding="utf-8-sig"))
    old_command = baseline["command"]
    if baseline["exit_code"] != 0 or len(old_command) != 4 or old_command[1] != "--beamng-replacement":
        raise ValueError("Expected successful full-vehicle replacement baseline timing")
    source = Path(old_command[2]).resolve()
    old_exe, new_exe = Path(old_command[0]).resolve(), args.new_exe.resolve()
    output = args.output.resolve()
    if output.exists():
        raise FileExistsError(f"Output must be new: {output}")
    report_path = (args.report or output.with_suffix(".benchmark.json")).resolve()
    if args.baseline_zip:
        reference = args.baseline_zip.resolve()
    else:
        candidates = list(Path(old_command[3]).glob("*.zip"))
        if len(candidates) != 1:
            raise ValueError("Specify --baseline-zip when the baseline folder has not exactly one ZIP")
        reference = candidates[0].resolve()
    old_hash, new_hash, source_hash = digest(old_exe), digest(new_exe), digest(source)
    if old_hash != baseline["executable_sha256"]:
        raise ValueError("Archived executable does not match baseline receipt")
    if marker(reference)["source_archive_sha256"] != source_hash:
        raise ValueError("Source copy no longer matches the baseline export")
    command = [str(new_exe), old_command[1], str(source), str(output)]
    with report_path.open("x", encoding="utf-8") as receipt:
        started = time.perf_counter()
        process = subprocess.run(command, capture_output=True, text=True, encoding="utf-8-sig",
                                 errors="replace", timeout=1800)
        elapsed = time.perf_counter() - started
        comparison = None
        failure = None
        try:
            if process.returncode:
                raise RuntimeError(f"Current export failed with exit code {process.returncode}")
            outputs = list(output.glob("*.zip"))
            if len(outputs) != 1:
                raise ValueError("Expected exactly one output ZIP")
            comparison = compare(reference, outputs[0])
            if digest(source) != source_hash:
                raise ValueError("Source copy changed during export")
            if digest(new_exe) != new_hash:
                raise ValueError("Current executable changed during benchmark")
        except Exception as error:
            failure = str(error)
        passed = failure is None and comparison is not None and comparison["passed"]
        report = {
            "command": command, "elapsed_seconds": elapsed,
            "baseline_seconds": baseline["elapsed_seconds"],
            "speedup": baseline["elapsed_seconds"] / elapsed,
            "baseline_executable_sha256": old_hash, "current_executable_sha256": new_hash,
            "source_sha256": source_hash, "exit_code": process.returncode,
            "stdout": process.stdout, "stderr": process.stderr,
            "comparison": comparison, "failure": failure, "passed": passed,
        }
        json.dump(report, receipt, indent=2)
        receipt.write("\n")
    print(json.dumps({"report": str(report_path), "passed": passed, "seconds": elapsed,
                      "speedup": report["speedup"]}))
    if not passed:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
