import os
import subprocess
from datetime import datetime, timedelta, timezone
from uuid import uuid4

from utils import assert_access_token_invalid, c, create_account, create_admin_account, get_self, make_client, save_auth

login = create_account("a", "a@a", "a")
sessions = [login["session"]]

# get current session
resp = c.get("/auth/session")
assert resp.status_code == 200
assert resp.json() == login["session"]

# login by username
resp = c.post("/auth/sessions", json={"name_or_email": "A", "password": "a"})
assert resp.status_code == 200
login = resp.json()
sessions.append(login["session"])

# login by email
resp = c.post("/auth/sessions", json={"name_or_email": "A@a", "password": "a"})
assert resp.status_code == 200
login = resp.json()
sessions.append(login["session"])

## invalid username
resp = c.post("/auth/sessions", json={"name_or_email": "x", "password": "a"})
assert resp.status_code == 401
assert resp.json() == {"detail": "Invalid credentials"}

## invalid password
resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "x"})
assert resp.status_code == 401
assert resp.json() == {"detail": "Invalid credentials"}

## recaptcha
for _ in range(2):
    resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "x"})
    assert resp.status_code == 401
    assert resp.json() == {"detail": "Invalid credentials"}

resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "x"})
assert resp.status_code == 412
assert resp.json() == {"detail": "Recaptcha failed"}

resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "x", "recaptcha_response": "success-0.3"})
assert resp.status_code == 412
assert resp.json() == {"detail": "Recaptcha failed"}

resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "x", "recaptcha_response": "success-0.7"})
assert resp.status_code == 401
assert resp.json() == {"detail": "Invalid credentials"}

resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "x"})
assert resp.status_code == 412
assert resp.json() == {"detail": "Recaptcha failed"}

resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "x", "recaptcha_response": "success-0.7"})
assert resp.status_code == 401
assert resp.json() == {"detail": "Invalid credentials"}

## the login is locked after five failed attempts, whatever the captcha says.
## The five above were: one without a captcha, two more, and the two that were
## answered with a valid captcha response.
resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "a", "recaptcha_response": "success-0.7"})
assert resp.status_code == 429
assert resp.json() == {"detail": "Too many failed login attempts"}
retry_after = int(resp.headers["retry-after"])
assert 0 < retry_after <= 60
## the web interface is served from another origin than this api, so the header
## has to be exposed explicitly for a browser to be able to read it
assert "retry-after" in resp.headers["access-control-expose-headers"].lower()

## the lock belongs to the account, not to the spelling of the login
resp = c.post("/auth/sessions", json={"name_or_email": "a@a", "password": "a", "recaptcha_response": "success-0.7"})
assert resp.status_code == 429
assert resp.json() == {"detail": "Too many failed login attempts"}

## it ends on its own; the captcha is still asked for, because that counter is
## a different one and is only cleared by a successful login
os.system("date -s '+2min'")

resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "a"})
assert resp.status_code == 412
assert resp.json() == {"detail": "Recaptcha failed"}

resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "a", "recaptcha_response": "success-0.7"})
assert resp.status_code == 200
login = resp.json()
sessions.append(login["session"])

for _ in range(3):
    resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "x"})
    assert resp.status_code == 401
    assert resp.json() == {"detail": "Invalid credentials"}

resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "a"})
assert resp.status_code == 412
assert resp.json() == {"detail": "Recaptcha failed"}

resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "a", "recaptcha_response": "success-0.7"})
assert resp.status_code == 200
login = resp.json()
sessions.append(login["session"])

## disabled
# The legacy CLI shortcut cannot manufacture an unreasoned restriction.
legacy = subprocess.run(
    ["academy", "admin", "user", "create", "--disabled", "b", "b@b", "b"], capture_output=True, text=True
)
assert legacy.returncode != 0
assert "complete moderation decision" in legacy.stderr
restricted_client = make_client()
restricted = create_account("b", "b@b", "b", restricted_client)
moderator = make_client()
create_admin_account("moderator", "moderator@example.com", "moderator", moderator)
case_id = str(uuid4())
opened = moderator.post(
    "/auth/moderation/admin/open",
    json={
        "id": case_id,
        "target_id": restricted["user"]["id"],
        "source": "own_review",
        "private_evidence": {"facts": "Synthetic account-security scenario in disposable VM"},
    },
)
assert opened.status_code == 200, opened.text
decided = moderator.post(
    "/auth/moderation/admin/decide",
    json={
        "request_key": str(uuid4()),
        "case_id": case_id,
        "expected_revision": 0,
        "outcome": "restrict",
        "ends_at": (datetime.now(timezone.utc) + timedelta(days=1)).isoformat(),
        "misconduct_facts": "Synthetic independently verified account-security finding",
        "proportionality": "Synthetic limited restriction for this authentication test",
        "hearing": "Synthetic immediate account-security urgency assessed",
        "rationale": "Synthetic VM fixture only; no real-person determination",
        "ground": "Independent account-security fixture",
        "rule_version": "Synthetic security ground, not an AGB acceptance finding",
        "automation": "Explicit test command, no automatic merits decision",
        "scope": "Allgemeiner Kontozugang auf Bootstrap Academy; Rechtezugang bleibt erhalten",
        "redress": "Human review and other available remedies",
    },
)
assert decided.status_code == 200, decided.text
resp = c.post("/auth/sessions", json={"name_or_email": "b", "password": "b"})
assert resp.status_code == 403
assert resp.json() == {"detail": "User disabled"}

# list sessions
resp = c.get("/auth/sessions/me")
assert resp.status_code == 200
assert resp.json() == sessions

# impersonate
create_admin_account("admin", "admin@admin", "admin")
resp = c.post(f"/auth/sessions/{login['user']['id']}")
assert resp.status_code == 200

# refresh
resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "a"})
assert resp.status_code == 200
login = resp.json()
user = login["user"]
refresh_token = login["refresh_token"]
save_auth(login)

os.system("date -s '+10min'")
assert_access_token_invalid()

resp = c.put("/auth/session", json={"refresh_token": refresh_token})
assert resp.status_code == 200
login = resp.json()
save_auth(login)
assert get_self() == user

## cannot reuse
resp = c.put("/auth/session", json={"refresh_token": refresh_token})
assert resp.status_code == 401
assert resp.json() == {"detail": "Invalid refresh token"}

# logout current
resp = c.delete("/auth/session")
assert resp.status_code == 200
assert resp.json() is True

assert_access_token_invalid()
resp = c.put("/auth/session", json={"refresh_token": login["refresh_token"]})
assert resp.status_code == 401
assert resp.json() == {"detail": "Invalid refresh token"}

# logout all
resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "a"})
assert resp.status_code == 200
save_auth(login := resp.json())

resp = c.delete("/auth/sessions/me")
assert resp.status_code == 200
assert resp.json() is True

assert_access_token_invalid()
resp = c.put("/auth/session", json={"refresh_token": login["refresh_token"]})
assert resp.status_code == 401
assert resp.json() == {"detail": "Invalid refresh token"}

resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "a"})
assert resp.status_code == 200
save_auth(login := resp.json())

resp = c.get("/auth/sessions/me")
assert resp.status_code == 200
assert resp.json() == [login["session"]]

# logout other
resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "a"})
assert resp.status_code == 200
save_auth(login := resp.json())

x = make_client()
resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "a"})
assert resp.status_code == 200
save_auth(resp.json(), x)

resp = x.delete(f"/auth/sessions/{login['user']['id']}/{login['session']['id']}")
assert resp.status_code == 200
assert resp.json() is True

assert_access_token_invalid()
resp = c.put("/auth/session", json={"refresh_token": login["refresh_token"]})
assert resp.status_code == 401
assert resp.json() == {"detail": "Invalid refresh token"}
get_self(x)
