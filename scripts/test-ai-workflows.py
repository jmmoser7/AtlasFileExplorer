"""Run Slate's offline AI workflow matrix, retaining test names and failure logs.

    python scripts/test-ai-workflows.py
    python scripts/test-ai-workflows.py --executables path/to/package-binaries.json

The optional input maps package or package/integration-test names to executables.
The manifest also identifies separately invoked real provider and licensed native tests.
"""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
MATRIX = json.loads((ROOT / "scripts/ai-workflows.json").read_text(encoding="utf-8"))


def build(targets, out):
    packages = [key for key in targets if "/" not in key]
    actions = [["--lib", *(arg for pkg in packages for arg in ("-p", pkg))]]
    for package, tests in MATRIX["integrations"].items():
        actions.append(["-p", package, *(arg for test in tests for arg in ("--test", test))])
    executables = {}
    with (out / "build.log").open("w", encoding="utf-8") as log:
        for flags in actions:
            command = ["cargo", "test", "--locked", "--no-run", "--message-format=json", *flags]
            proc = subprocess.Popen(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=log,
                                    text=True, encoding="utf-8", errors="replace")
            for line in proc.stdout:
                message = json.loads(line)
                if message.get("reason") == "compiler-message":
                    log.write(message["message"].get("rendered") or "")
                if not message.get("executable") or not message.get("profile", {}).get("test"):
                    continue
                target = message["target"]
                key = target["name"].replace("_", "-")
                if "test" in target["kind"]:
                    package = Path(target["src_path"]).resolve().relative_to(ROOT).parts[1]
                    key = package + "/" + target["name"]
                if key in targets:
                    executables[key] = Path(message["executable"])
            if proc.wait():
                raise RuntimeError(f"Cargo build failed; see {log.name}")
    return executables


@contextmanager
def isolated_environment(exe=None):
    # Leave the CWD, compiler dependencies and system tools in place. The regular
    # Rust app harness also pins subscription packs and suppresses catalog probes.
    with tempfile.TemporaryDirectory(prefix="s-ai-") as directory:
        root = Path(directory)
        profile = root / "p"
        local = profile / "AppData/Local"
        folders = {"HOME": profile, "USERPROFILE": profile, "LOCALAPPDATA": local,
                   "APPDATA": profile / "AppData/Roaming", "XDG_CONFIG_HOME": profile / "config",
                   "XDG_DATA_HOME": profile / "data", "XDG_CACHE_HOME": profile / "cache",
                   "CODEX_HOME": profile / ".codex", "ATLAS_SECRET_DIR": root / "secrets",
                   "TMP": root / "t", "TEMP": root / "t", "TMPDIR": root / "t"}
        for folder in folders.values():
            folder.mkdir(parents=True, exist_ok=True)
        env = os.environ.copy()
        for name in ("CURSOR_API_KEY", "OPENAI_API_KEY", "ANTHROPIC_API_KEY", "CODEX_BIN",
                     "ATLAS_CURSOR_AGENT", "ATLAS_CLAUDE_BIN", "COMFY_BIN", "SLATE_LIVE_CODEX",
                     "ATLAS_CODEX_LIVE_SMOKE"):
            env.pop(name, None)
        env.update({name: str(folder) for name, folder in folders.items()})
        env["ATLAS_CURSOR_KEY_PATH"] = str(root / "absent-cursor-key")
        if os.name == "nt" and exe:
            marker = local / "NativeFileAtlas/slate-assoc.txt"
            marker.parent.mkdir()
            marker.write_bytes(str(exe.resolve()).encode("utf-8"))
        yield env


def discover(exe, cwd, env, ignored=False):
    args = [str(exe), "--list"]
    if ignored:
        args.append("--ignored")
    listing = subprocess.check_output(args, cwd=cwd, env=env, text=True, encoding="utf-8")
    return {line.removesuffix(": test") for line in listing.splitlines() if line.endswith(": test")}


def batches(tests):
    # libtest accepts several exact names, but CreateProcess limits command lines.
    # These fixtures replace and remove their process secret-store override.
    # Keep their teardown from affecting any subsequent or concurrent case.
    isolated = {name for name in tests if name.rsplit("::", 1)[-1] in MATRIX["separate_process"]}
    batch = []
    size = 0
    for test in sorted(tests - isolated):
        if size + len(test) > 22000:
            yield batch
            batch = []
            size = 0
        batch.append(test)
        size += len(test) + 1
    if batch:
        yield batch
    for test in sorted(isolated):
        yield [test]


def run_package(target, exe, out, threads, env):
    package = target.split("/")[0]
    cwd = ROOT / ("apps/slate" if package == "slate" else "crates/" + package)
    names = discover(exe, cwd, env)
    ignored = discover(exe, cwd, env, True)
    available = names - ignored
    if package == "slate":
        groups = {group: sorted(n for n in available if any(n.startswith(p) for p in prefixes))
                  for group, prefixes in MATRIX["slate"].items()}
        selected = set().union(*groups.values())
    else:
        groups = {MATRIX["adapter_crates"][package]: sorted(available)}
        selected = available
    missing = [group for group, tests in groups.items() if not tests]
    for name in MATRIX["required_tests"].get(package, []):
        if not any(t.endswith("::" + name) or t == name for t in selected):
            missing.append(name)
    if not selected:
        raise RuntimeError(f"{package}: no matching tests were discovered")
    with exe.open("rb") as image:
        checksum = hashlib.sha256()
        for chunk in iter(lambda: image.read(1024 * 1024), b""):
            checksum.update(chunk)
        digest = checksum.hexdigest()
    passed = set()
    failed = set()
    code = 0
    log_path = out / (target.replace("/", "-") + ".log")
    with log_path.open("w", encoding="utf-8") as log:
        for batch in batches(selected):
            run = subprocess.run([str(exe), "--exact", *batch, "--test-threads", str(threads)],
                                 cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                 text=True, encoding="utf-8", errors="replace")
            log.write(run.stdout)
            code = code or run.returncode
            for name, result in re.findall(r"^test (\S+) \.\.\. (ok|FAILED)$", run.stdout, re.M):
                (passed if result == "ok" else failed).add(name)
    return {"executable": str(exe), "sha256": digest, "groups": groups, "passed": len(passed),
            "passed_names": sorted(passed),
            "failed": sorted(failed), "unaccounted": sorted(selected - passed - failed),
            "ignored": sorted(ignored), "missing": missing,
            "returncode": code or int(bool(missing)), "log": str(log_path)}


def evidence(entry, patterns):
    def matches(name):
        return any(re.search(pattern, name) for pattern in patterns)

    return {outcome: sorted(name for name in entry.get(source, []) if matches(name))
            for outcome, source in (("passed", "passed_names"), ("failed", "failed"),
                                    ("unaccounted", "unaccounted"))}


def coverage(report, observations):
    rows = {}
    for workflow, spec in MATRIX["entrypoints"].items():
        shared = evidence(report.get("slate", {}), spec["app"])
        adapters = {}
        for provider in spec["adapters"]:
            package = MATRIX["providers"][provider]
            tested = evidence(report.get(package, {}), spec["transport"][package])
            live = [item for item in observations if item["entrypoint"] == workflow
                    and item["adapter"] == provider]
            adapters[provider] = {"target": package, "contract": tested, "live": live}
        rows[workflow] = {"description": spec["description"], "slate": shared, "adapters": adapters}
    return rows


def live_receipts(path):
    if path is None:
        return []
    observations = json.loads(path.read_text(encoding="utf-8-sig"))
    for item in observations:
        route = MATRIX["entrypoints"][item["entrypoint"]]
        if item["adapter"] not in route["adapters"] or item["status"] not in ("passed", "failed", "blocked"):
            raise ValueError(f"Unknown route, adapter or live status in {item}")
        if not item.get("detail") or not item.get("files"):
            raise ValueError("A live observation requires a description and saved evidence")
        for source in item["files"]:
            if not (path.parent / source).is_file():
                raise ValueError(f"Missing live evidence file: {source}")
    return observations


def write_summary(rows, out):
    lines = ["# AI workflow verification", "",
             "App and transport counts are shared offline regression cases. "
             "Each live observation is listed independently in entrypoint-coverage.json.", "",
             "| Entry point | Adapter | Passing app cases | Passing transport cases | Live |",
             "| --- | --- | ---: | ---: | --- |"]
    for name, row in rows.items():
        for provider, entry in row["adapters"].items():
            status = ", ".join(sorted({r["status"] for r in entry["live"]})) or "Unmeasured"
            lines.append(f"| {name} | {provider} | {len(row['slate']['passed'])} | "
                         f"{len(entry['contract']['passed'])} | {status} |")
    (out / "summary.md").write_text("\n".join(lines) + "\n", encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executables", type=Path)
    parser.add_argument("--live-receipts", type=Path, help="Optional JSON observations referencing local evidence")
    parser.add_argument("--output", type=Path, default=ROOT / "target/ai-workflows")
    parser.add_argument("--test-threads", type=int, default=1)
    options = parser.parse_args()
    observations = live_receipts(options.live_receipts)
    if options.test_threads < 1:
        parser.error("--test-threads must be positive")
    out = options.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    packages = [pkg for pkg in MATRIX["adapter_crates"] if pkg not in MATRIX["integrations"]]
    packages += [pkg + "/" + case for pkg, tests in MATRIX["integrations"].items() for case in tests]
    packages.append("slate")
    if options.executables:
        data = json.loads(options.executables.read_text(encoding="utf-8-sig"))
        executables = {key: Path(value).resolve() for key, value in data.items()}
    else:
        executables = build(packages, out)
    report = {}

    def record(name, entry):
        report[name] = entry
        (out / "results.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(name, {k: entry[k] for k in ("passed", "failed", "missing", "returncode", "error") if k in entry},
              flush=True)

    for package in packages:
        try:
            exe = executables[package]
            with isolated_environment(exe) as env:
                record(package, run_package(package, exe, out, options.test_threads, env))
        except (OSError, KeyError, subprocess.SubprocessError, RuntimeError) as error:
            record(package, {"error": str(error), "returncode": 1})
    js_tests = sorted(ROOT.glob(MATRIX["node_tests"]))
    try:
        if not js_tests:
            raise RuntimeError("no Cursor sidecar tests were discovered")
        with isolated_environment() as env, (out / "cursor-sidecar.log").open("w", encoding="utf-8") as log:
            help_text = subprocess.check_output(["node", "--help"], env=env, text=True, encoding="utf-8")
            reporter = ["--test-reporter=tap"] if "--test-reporter" in help_text else []
            js = subprocess.run(["node", "--test", *reporter, *(p.name for p in js_tests)],
                                cwd=js_tests[0].parent, env=env, stdout=log, stderr=subprocess.STDOUT)
        summary = (out / "cursor-sidecar.log").read_text(encoding="utf-8", errors="replace")
        # Node's TAP reporter lists nested test names; files are a separate discovery inventory.
        passed = [name for name in re.findall(r"^\s*ok \d+ - (.+)$", summary, re.M)
                  if not name.endswith(".test.mjs")]
        failed = re.findall(r"^\s*not ok \d+ - (.+)$", summary, re.M)
        record("cursor-sidecar", {"files": [p.name for p in js_tests], "passed": len(passed),
                                  "passed_names": passed, "failed": failed, "returncode": js.returncode})
    except (OSError, RuntimeError) as error:
        record("cursor-sidecar", {"error": str(error), "returncode": 1})
    rows = coverage(report, observations)
    (out / "entrypoint-coverage.json").write_text(json.dumps(rows, indent=2), encoding="utf-8")
    write_summary(rows, out)
    gaps = [f"{name}/Slate" for name, row in rows.items() if not row["slate"]["passed"]]
    gaps += [f"{name}/{provider}" for name, row in rows.items() for provider, adapter in row["adapters"].items()
             if not adapter["contract"]["passed"]]
    if gaps:
        print("Missing entrypoint evidence:", ", ".join(gaps))
    return int(bool(gaps) or any(r["returncode"] or r.get("failed") or r.get("unaccounted") for r in report.values()))


if __name__ == "__main__":
    sys.exit(main())
