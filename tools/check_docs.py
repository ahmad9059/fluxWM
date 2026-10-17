#!/usr/bin/env python3
"""Check documentation artifacts. Does not test or certify a compositor."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[1]


def check(source_cache: Path | None) -> dict:
    checks: dict = {"scope": "Documentation integrity and explicit Lua 5.4 example checks; no FluxWM runtime tests"}
    docs = []
    for directory, subdirectories, files in os.walk(ROOT):
        # Build output can contain API docs and incomplete frozen review trees.
        # Validate authored repository documents, not generated/cache copies.
        subdirectories[:] = [name for name in subdirectories if name not in {".git", "target"}]
        docs.extend(Path(directory) / name for name in files if name.endswith(".md"))
    docs.sort()
    checks["excluded_directories"] = [".git", "target"]
    errors = []
    local_links = 0
    for path in docs:
        # This repository uses ordinary inline Markdown links with no parentheses in paths.
        # This is not a general-purpose CommonMark parser or anchor validator.
        for target in re.findall(r"\[[^\]]*\]\(([^)]+)\)", path.read_text()):
            if re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:", target) or target.startswith("#"):
                continue
            relative = unquote(target.split("#", 1)[0].strip("<>"))
            if not relative:
                continue
            local_links += 1
            resolved = (path.parent / relative).resolve()
            if not resolved.is_relative_to(ROOT) or not resolved.exists():
                errors.append(f"Invalid local link: {path.relative_to(ROOT)} -> {target}")
    checks.update(markdown_documents=len(docs), markdown_words=sum(len(p.read_text().split()) for p in docs), local_links=local_links)
    phases = sorted((ROOT / "docs/phases").glob("*.md"))
    if len(phases) != 10:
        errors.append(f"Expected 10 phases, got {len(phases)}")
    for index, path in enumerate(phases, 1):
        body = path.read_text()
        for required in [f"# Phase {index}:", "Dependencies:", "## Implementation tasks", "## Required artifacts", "## Verification procedure", "## Exit gate", "## Production audit supplement"]:
            if required not in body:
                errors.append(f"Missing {required}: {path.relative_to(ROOT)}")
    checks["phase_documents"] = len(phases)
    manifest = json.loads((ROOT / "docs/evidence/upstream-sources.json").read_text())
    sources = {}
    cache_count = 0
    retrieved_count = 0
    for repo in manifest["repositories"]:
        sha = repo.get("resolved_commit", "")
        if not re.fullmatch(r"[0-9a-f]{40}", sha):
            errors.append(f"Invalid commit: {repo['id']}")
        for entry in repo["files"]:
            if entry["status"] != "retrieved":
                continue
            retrieved_count += 1
            if f"/{sha}/" not in entry["url"] or not re.fullmatch(r"[0-9a-f]{64}", entry["sha256"]):
                errors.append(f"Unpinned or unhashed source: {entry['path']}")
            sources[entry["browse_url"]] = entry["sha256"]
            if source_cache is not None:
                cache_file = source_cache / repo["id"] / entry["path"]
                if not cache_file.exists():
                    errors.append(f"Missing source-cache file: {cache_file}")
                elif hashlib.sha256(cache_file.read_bytes()).hexdigest() != entry["sha256"]:
                    errors.append(f"Source hash mismatch: {cache_file}")
                else:
                    cache_count += 1
    inventory = json.loads((ROOT / "docs/evidence/protocol-inventory.json").read_text())
    for entry in inventory:
        if sources.get(entry["source"]) != entry["source_sha256"]:
            errors.append(f"Unmatched protocol source: {entry['interface']}")
        if entry["fluxwm_advertised_version"] is not None or entry["implementation_status"] != "not-implemented":
            errors.append(f"Unexpected implementation claim: {entry['interface']}")
    checks.update(source_repositories=len(manifest["repositories"]), retrieved_source_files=retrieved_count, source_cache_hashes_checked=cache_count, protocol_interfaces=len(inventory))
    probe_record = json.loads((ROOT / "docs/evidence/mlua-probe-result.json").read_text())
    for relative, expected_hash in probe_record["files_sha256"].items():
        probe_path = ROOT / relative
        if not probe_path.exists() or hashlib.sha256(probe_path.read_bytes()).hexdigest() != expected_hash:
            errors.append(f"Evidence-probe source changed: {relative}")
    probe_stdout = (ROOT / "docs/evidence/mlua-probe-output.txt").read_text()
    if probe_record["exit_code"] != 0 or probe_stdout != probe_record["stdout"]:
        errors.append("Recorded mlua execution output is inconsistent")
    checks["mlua_probe_record_integrity"] = "checked; recorded execution is not rerun by this command"
    cases = json.loads((ROOT / "docs/evidence/edge-cases.json").read_text())
    schedule = json.loads((ROOT / "docs/evidence/phase-gates.json").read_text())
    features = schedule["feature_first_phase"]
    ids = [case["id"] for case in cases]
    if len(cases) < 80 or len(set(ids)) != len(ids):
        errors.append("Expected at least 80 uniquely identified specified cases")
    catalog = (ROOT / "docs/15-edge-cases.md").read_text()
    scheduled_items = {}
    for phase, gate_ids in schedule["gates"].items():
        for case_id in gate_ids:
            if case_id in scheduled_items:
                errors.append(f"Duplicate gate assignment: {case_id}")
            scheduled_items[case_id] = int(phase)
    known_ids = set()
    for case in cases:
        intro, qualify = case["introduced_phase"], case["qualified_phase"]
        if not 1 <= intro <= qualify <= 10:
            errors.append(f"Invalid introduction/qualification phases: {case['id']}")
        expected_row = f"| {case['id']} | {intro} / {qualify} | {case['stimulus']} | {case['expected']} | {case['evidence_required']} |"
        if expected_row not in catalog:
            errors.append(f"Markdown/JSON row mismatch: {case['id']}")
        for item in [case] + case["variants"]:
            known_ids.add(item["id"])
            if item["status"] != "specified" or item["result"] is not None:
                errors.append(f"Runtime result requires an evidence-aware checker update: {item['id']}")
            if scheduled_items.get(item["id"]) != item["qualified_phase"]:
                errors.append(f"Qualification gate mismatch: {item['id']}")
            for feature in item["required_features"]:
                if feature not in features or features[feature] > item["qualified_phase"]:
                    errors.append(f"Gate precedes required feature {feature}: {item['id']}")
            if item is not case and f"| {item['id']} | variant / {item['qualified_phase']} | Narrow early variant | {item['expected']} |" not in catalog:
                errors.append(f"Variant Markdown/JSON mismatch: {item['id']}")
        for field in ["stimulus", "expected", "evidence_required"]:
            if not case[field]:
                errors.append(f"Empty {field}: {case['id']}")
    if set(scheduled_items) != known_ids:
        errors.append("Gate schedule and catalog IDs differ")
    checks["specified_acceptance_cases"] = len(cases)
    checks["narrow_early_variants"] = sum(len(c["variants"]) for c in cases)
    checks["phase_feature_coherence"] = "checked against declared feature-first-phase map; runtime existence unproven"
    lua = shutil.which("lua5.4")
    luac = shutil.which("luac5.4")
    if not lua or not luac:
        errors.append("Explicit lua5.4 and luac5.4 are required for the target-runtime example check")
    else:
        commands = [
            [luac, "-p", "docs/examples/config.lua"],
            [luac, "-p", "docs/examples/nested-config.lua"],
            [lua, "-e", 'local c=assert(loadfile("docs/examples/nested-config.lua","t",{}))(); assert(type(c)=="table" and c.schema_version==1 and #c.bindings==16)'],
            [lua, "-e", 'local c=assert(loadfile("docs/examples/config.lua","t",{}))(); assert(type(c)=="table" and c.schema_version==1)'],
            [lua, "docs/evidence/lua-hook-catchability.lua"],
            [lua, "docs/examples/check-config.lua"],
        ]
        results = []
        for command in commands:
            try:
                result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, timeout=5)
                results.append({"command": command, "exit_code": result.returncode, "stdout": result.stdout.strip(), "stderr": result.stderr.strip()})
                if result.returncode:
                    errors.append(f"Lua check failed: {command}")
            except subprocess.TimeoutExpired:
                errors.append(f"Lua check exceeded 5 seconds: {command}")
        checks["lua54_checks"] = results
    checks["errors"] = errors
    checks["passed"] = not errors
    checks["limits"] = ["Does not validate external URL availability or all Markdown anchors", "Source hashes prove recorded bytes, not build/API interoperability", "Standalone Lua checks do not validate mlua embedding or a FluxWM schema parser", "Acceptance cases are specifications only; no runtime passed status", "Feature phase map is a reviewed declaration, not implementation evidence", "Fixture schema checks cover the documented example contract, not a hardened production parser", "Does not prove production completeness or hardware compatibility"]
    return checks


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-cache", type=Path, help="Optional temporary cache organized by manifest repository ID and path")
    parser.add_argument("--report", type=Path, help="Write the check report as JSON")
    args = parser.parse_args()
    report = check(args.source_cache)
    if args.report:
        args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
