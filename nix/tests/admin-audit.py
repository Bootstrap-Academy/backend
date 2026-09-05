import os
import subprocess
import time

from utils import c, create_account, make_client, save_auth, setup_mfa, wait_for_new_totp_window

ADMIN_PASSWORD = "supersecureadminpassword"


def audit_log(**params):
    resp = c.get("/admin/audit-log", params=params)
    assert resp.status_code == 200
    return resp.json()


def db_entry_count():
    status, out = subprocess.getstatusoutput(
        "sudo -u postgres psql -t --csv academy <<< 'select count(*) from admin_audit_log'"
    )
    assert status == 0
    return int(out.strip())


# an ordinary user for the administrator to act on
u = make_client()
user = create_account("user", "user@example.com", "userpassword", u)["user"]

# an administrator that has not set up a second factor yet
os.system(f"academy admin user create --admin --verified admin admin@example.com {ADMIN_PASSWORD}")
resp = c.post("/auth/sessions", json={"name_or_email": "admin", "password": ADMIN_PASSWORD})
assert resp.status_code == 200
login = resp.json()
assert login["user"]["admin"] is True
assert login["session"]["mfa_verified"] is False
save_auth(login)

# without a second factor the session does not grant administrative privileges
resp = c.get("/auth/users")
assert resp.status_code == 403
assert resp.json() == {"detail": "Admin MFA required"}

resp = c.get("/admin/audit-log")
assert resp.status_code == 403
assert resp.json() == {"detail": "Admin MFA required"}

resp = c.patch(f"/auth/users/{user['id']}", json={"enabled": False})
assert resp.status_code == 403
assert resp.json() == {"detail": "Admin MFA required"}
rejected_request_id = resp.headers["X-Request-Id"]

# ... but the account can still be used to set up the second factor
totp = setup_mfa()
wait_for_new_totp_window()

resp = c.post("/auth/sessions", json={"name_or_email": "admin", "password": ADMIN_PASSWORD, "mfa_code": totp.now()})
assert resp.status_code == 200
login = resp.json()
assert login["session"]["mfa_verified"] is True
save_auth(login)
admin = login["user"]

resp = c.get("/auth/users")
assert resp.status_code == 200

# the rejected request has been recorded as well
log = audit_log(target_user_id=user["id"])
assert log["total"] == 1
entry = log["entries"][0]
assert entry["method"] == "PATCH"
assert entry["path"] == f"/auth/users/{user['id']}"
assert entry["admin_user_id"] == admin["id"]
assert entry["target_user_id"] == user["id"]
assert entry["status"] == 403
assert entry["request_id"] == rejected_request_id

# reading data is not recorded
before = audit_log()["total"]
assert c.get(f"/auth/users/{user['id']}").status_code == 200
assert audit_log()["total"] == before

# every state changing request is
resp = c.patch(f"/auth/users/{user['id']}", json={"display_name": "Renamed"})
assert resp.status_code == 200

log = audit_log(target_user_id=user["id"])
assert log["total"] == 2
entry = log["entries"][0]
assert entry["method"] == "PATCH"
assert entry["path"] == f"/auth/users/{user['id']}"
assert entry["admin_user_id"] == admin["id"]
assert entry["target_user_id"] == user["id"]
assert entry["status"] == 200
assert entry["request_id"] == resp.headers["X-Request-Id"]

# most recent first
assert log["entries"][1]["status"] == 403

# the affected user is taken from the matched route, not just from /auth/users
resp = c.post(f"/auth/sessions/{user['id']}")
assert resp.status_code == 200
log = audit_log(admin_user_id=admin["id"])
assert log["entries"][0]["path"] == f"/auth/sessions/{user['id']}"
assert log["entries"][0]["target_user_id"] == user["id"]

# requests of ordinary users are not recorded
before = audit_log()["total"]
resp = u.patch("/auth/users/me", json={"display_name": "Selfnamed"})
assert resp.status_code == 200
assert audit_log()["total"] == before

# the audit log itself requires admin privileges
resp = u.get("/admin/audit-log")
assert resp.status_code == 403
assert resp.json() == {"detail": "Permission denied"}

# entries are kept for twelve months
entries = db_entry_count()
assert entries == before

os.system("systemctl start academy-task-prune-database.service")
time.sleep(1)
assert db_entry_count() == entries

os.system("date -s '+400days'")
time.sleep(0.5)
os.system("systemctl start academy-task-prune-database.service")
time.sleep(1)
assert db_entry_count() == 0
