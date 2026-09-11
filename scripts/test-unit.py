"""Run ordinary Rust checks with an owned PostgreSQL 18 fixture."""

import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import tempfile
import uuid


# Exact inputs formerly interpreted by the integration tests, not toolchain namespaces.
HISTORICAL_FIXTURE_ENV = (
    "BOOTSTRAP_DETERMINATION_FIXTURE",
    "BOOTSTRAP_HOLD_REVIEW_FIXTURE",
    "BOOTSTRAP_IF1_FIXTURE",
    "BOOTSTRAP_INVENTORY_FIXTURE",
    "BOOTSTRAP_INVOICE_PRESERVATION_FIXTURE",
    "BOOTSTRAP_L3_LEARNING_FIXTURE",
    "BOOTSTRAP_LEARNING_START_BASELINE",
    "BOOTSTRAP_PERSONAL_PURCHASE_FIXTURE",
    "BOOTSTRAP_RETENTION_PAGING_FIXTURE",
    "BOOTSTRAP_STAFF_READS_FIXTURE",
    "BOOTSTRAP_WALLET_RED",
    "BOOTSTRAP_WALLET_TARGET_FIXTURE",
    "IF1_BASELINE",
    "IF1_RESIDUAL_BASELINE",
    "INVOICE_SOURCE_BASELINE",
)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-dir", type=Path, required=True)
    parser.add_argument("--suite", choices=["unit", "postgres"], default="unit")
    args = parser.parse_args()
    if os.geteuid() == 0:
        parser.error("run as an unprivileged user")
    evidence = args.evidence_dir.resolve()
    evidence.mkdir(mode=0o700, parents=True, exist_ok=False)
    repo = Path(__file__).resolve().parent.parent
    pg_ctl_path = shutil.which("pg_ctl")
    if pg_ctl_path is None:
        parser.error("PostgreSQL 18 tools must be on PATH")
    pg_bin = Path(pg_ctl_path).resolve().parent
    for tool in ["postgres", "initdb", "pg_ctl", "createdb", "pg_dump"]:
        version = subprocess.check_output([str(pg_bin / tool), "--version"], text=True)
        if "(PostgreSQL) 18." not in version:
            parser.error(f"{tool} must come from the same PostgreSQL 18 installation")
    config = (repo / "config.dev.toml").read_text()
    old_url = "postgres://academy@127.0.0.1:5432/academy"
    if config.count(old_url) != 1:
        raise RuntimeError("the committed synthetic development database URL must occur exactly once")
    env = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith("PG")
        and key not in {"DATABASE_URL", "ACADEMY_CONFIG", "ACADEMY_UNIT_TEST_FIXTURE", "ACADEMY_UNIT_TEST_RUN_ID"}
    }
    env.update(SQLX_OFFLINE="true", RUST_TEST_THREADS="1")
    forbidden = [key for key in HISTORICAL_FIXTURE_ENV if key in env]
    if args.suite == "postgres" and forbidden:
        parser.error("historical fixture inputs are not CI inputs: " + ", ".join(forbidden))
    root = Path(tempfile.mkdtemp(prefix="academy-unit-tests-", dir="/tmp")).resolve()
    data = root / "pgdata"
    (root / "evidence").mkdir(mode=0o700)
    (root / "socket").mkdir(mode=0o700)
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    marker = {
        "unit": "academy-unit-tests",
        "canonical_root": str(root),
        "owner_uid": os.geteuid(),
        "run_id": str(uuid.uuid4()),
        "port": port,
        "role": "academy_unit_tests",
        "database": "academy_unit_tests",
        "pg_bin": str(pg_bin),
        "suite": args.suite,
    }
    marker_bytes = (json.dumps(marker, indent=2) + "\n").encode()
    owner = root / "OWNER.json"
    owner.write_bytes(marker_bytes)
    owner.chmod(0o600)
    config = config.replace(old_url, f"postgres://academy_unit_tests@127.0.0.1:{port}/academy_unit_tests")
    fixture = root / "fixture.toml"
    fixture.write_text(config)
    fixture.chmod(0o600)
    env.update(
        ACADEMY_CONFIG=str(fixture), ACADEMY_UNIT_TEST_FIXTURE=str(root), ACADEMY_UNIT_TEST_RUN_ID=marker["run_id"]
    )
    steps = []

    def run(name: str, argv: list[str]) -> int:
        print(f"RUN {name}", flush=True)
        with (evidence / f"{name}.log").open("wb") as log:
            process = subprocess.Popen(
                argv, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True
            )
            try:
                code = process.wait(timeout=1800)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
                code = 124
        steps.append({"name": name, "argv": argv, "exit": code})
        print((evidence / f"{name}.log").read_text(errors="replace"), end="", flush=True)
        return code

    start_attempted = False
    result = 1
    try:
        if run(
            "initdb",
            [
                str(pg_bin / "initdb"),
                "-D",
                str(data),
                "-U",
                "academy_unit_tests",
                "--auth=trust",
                "--no-locale",
                "--encoding=UTF8",
                *(["--locale-provider=icu", "--icu-locale=de-DE"] if args.suite == "postgres" else []),
            ],
        ):
            return 1
        with (data / "postgresql.conf").open("a") as config_file:
            config_file.write(
                f"\nlisten_addresses = '127.0.0.1'\nport = {port}\n" f"unix_socket_directories = '{root / 'socket'}'\n"
            )
            if args.suite == "postgres":
                config_file.write("statement_timeout = '60s'\n")
        start_attempted = True
        if run("start", [str(pg_bin / "pg_ctl"), "-D", str(data), "-l", str(root / "postgres.log"), "-w", "start"]):
            return 1
        if run(
            "createdb",
            [
                str(pg_bin / "createdb"),
                "-h",
                "127.0.0.1",
                "-p",
                str(port),
                "-U",
                "academy_unit_tests",
                "academy_unit_tests",
            ],
        ):
            return 1
        if args.suite == "postgres":
            result = run(
                "postgres",
                [
                    "cargo",
                    "test",
                    "-p",
                    "academy_persistence_postgres",
                    "--no-fail-fast",
                    "--all-features",
                    "--test",
                    "*",
                    "--",
                    "--nocapture",
                ],
            )
            return result
        result = run("unit", ["cargo", "test", "--no-fail-fast", "--all-features", "--bins", "--lib"])
        if result == 0:
            result = run("doc", ["cargo", "test", "--no-fail-fast", "--all-features", "--doc"])
        return result
    finally:
        # Refuse cleanup if this process's exact ownership record was replaced.
        if root.resolve() != root or root.stat().st_uid != os.geteuid() or owner.read_bytes() != marker_bytes:
            raise RuntimeError("fixture ownership changed; refusing cleanup")
        if start_attempted and (data / "postmaster.pid").exists():
            if run("stop", [str(pg_bin / "pg_ctl"), "-D", str(data), "-m", "fast", "-w", "stop"]):
                raise RuntimeError(f"owned PostgreSQL did not stop; fixture retained at {root}")
        if (data / "postmaster.pid").exists():
            raise RuntimeError(f"owned PostgreSQL PID file remains; fixture retained at {root}")
        for source in [owner, fixture, root / "postgres.log"]:
            if source.exists():
                shutil.copyfile(
                    source, evidence / ("postgres-server.log" if source.name == "postgres.log" else source.name)
                )
        shutil.copytree(root / "evidence", evidence / "resets")
        shutil.rmtree(root)
        (evidence / "result.json").write_text(
            json.dumps(
                {"marker": marker, "steps": steps, "exit": result, "fixture_removed": not root.exists()}, indent=2
            )
            + "\n"
        )
        print(f"UNIT_TEST_EVIDENCE {evidence}", flush=True)


if __name__ == "__main__":
    raise SystemExit(main())
