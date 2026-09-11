"""L1 service interleavings against the owned T12_STATE synthetic backend/PG.

No existing deployment configuration or nonlocal network is used.
"""

from pathlib import Path

exec(
    compile(
        Path(__file__).with_name("contract-lifecycle.py").read_text().split("async def main():", 1)[0],
        str(Path(__file__).with_name("contract-lifecycle.py")),
        "exec",
    )
)
try:
    for args in [["migrate", "reset", "--force"], ["migrate", "up"], ["migrate", "demo", "--force"]]:
        subprocess.run(
            [str(ROOT / "target/debug/academy"), *args], env=env, cwd=ROOT, check=True, stdout=subprocess.DEVNULL
        )
    start()
    test_env = os.environ | {
        "T6_BACKEND_URL": "http://127.0.0.1:55872/shop/_internal",
        "T6_BACKEND_DB": "postgresql://morpheus@127.0.0.1:55572/t12backend",
    }
    for repo, database in [("skills-ms", "t12unused"), ("events-ms", "t12tests")]:
        test_env["T6_SOURCE_DB"] = (
            os.environ.get("T6_SKILLS_DB", f"postgresql+asyncpg://morpheus@127.0.0.1:55572/{database}")
            if repo == "skills-ms"
            else f"postgresql+asyncpg://morpheus@127.0.0.1:55572/{database}"
        )
        test_env["T6_EVENTS_DB"] = test_env["T6_SOURCE_DB"]
        tests = ["tests/services/test_purchase_contracts.py"] + (
            ["tests/services/test_booking_payments.py"] if repo == "events-ms" else []
        )
        subprocess.run(
            [str(ROOT.parent / repo / ".venv/bin/python"), "-m", "pytest", "-q", *tests],
            cwd=ROOT.parent / repo,
            env=test_env,
            check=True,
        )
finally:
    stop()
    cache.terminate()
    cache.wait(timeout=10)
    smtp.shutdown()
    smtp.server_close()
