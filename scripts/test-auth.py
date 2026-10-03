"""Run authentication regressions with owned loopback PostgreSQL 18 and Valkey.

Usage: python3 scripts/test-auth.py --evidence-dir /path/to/new/evidence
PostgreSQL 18 tools, valkey-server and cargo must be on PATH.
"""

import importlib.util
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import uuid

sys.dont_write_bytecode = True


def main(*, postgres_tests=None):
    os.umask(0o077)
    if os.geteuid() == 0:
        raise RuntimeError("run authentication tests as an unprivileged user")
    binary = shutil.which("valkey-server")
    if binary is None:
        raise RuntimeError("valkey-server must be on PATH")
    root = Path(tempfile.mkdtemp(prefix="academy-auth-tests-"))
    marker = json.dumps({"uid": os.geteuid(), "pid": os.getpid(), "run": str(uuid.uuid4())}).encode()
    (root / "OWNER.json").write_bytes(marker)
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    server = None
    try:
        with (root / "valkey.log").open("wb") as log:
            server = subprocess.Popen(
                [
                    binary,
                    "--bind",
                    "127.0.0.1",
                    "--port",
                    str(port),
                    "--save",
                    "",
                    "--appendonly",
                    "no",
                    "--dir",
                    str(root),
                ],
                stdout=log,
                stderr=log,
            )
        for _ in range(100):
            if server.poll() is not None:
                raise RuntimeError("owned Valkey exited before becoming ready")
            try:
                with socket.create_connection(("127.0.0.1", port), timeout=0.1):
                    break
            except OSError:
                time.sleep(0.05)
        else:
            raise RuntimeError("owned Valkey did not become ready")
        os.environ["AUTH_REVIEW_VALKEY_PORT"] = str(port)
        source = Path(__file__).resolve().with_name("test-unit.py")
        spec = importlib.util.spec_from_file_location("owned_postgres", source)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        sys.argv.extend(["--suite", "postgres"])
        return module.main(
            postgres_tests=postgres_tests
            or [("academy", "auth_robustness_native"), ("academy", "auth_robustness_logs")],
            cache_port=port,
        )
    finally:
        if server is not None and server.poll() is None:
            server.terminate()
            server.wait(timeout=10)
        if root.stat().st_uid != os.geteuid() or (root / "OWNER.json").read_bytes() != marker:
            raise RuntimeError("fixture ownership changed; refusing cleanup")
        shutil.rmtree(root)


if __name__ == "__main__":
    raise SystemExit(main())
