import re
import subprocess
import time

from utils import (
    assert_access_token_invalid,
    c,
    decode_mail_header,
    decode_mail_payload,
    discard_auth,
    fetch_mail,
    refresh_session,
    save_auth,
)

c.headers["User-Agent"] = "httpx test client"

# create
password = "my secure password"
req = {"name": "user", "display_name": "User 123", "email": "user@example.com", "password": password}

## recaptcha error
resp = c.post("/auth/users", json={**req, "recaptcha_response": "success-0.3"})
assert resp.status_code == 412
assert resp.json() == {"detail": "Recaptcha failed"}

## success
resp = c.post("/auth/users", json={**req, "recaptcha_response": "success-0.7"})
assert resp.status_code == 201
login = resp.json()
assert login == {
    "user": {
        "id": login["user"]["id"],
        "name": "user",
        "display_name": "User 123",
        "email": "user@example.com",
        "email_verified": False,
        "created_at": login["user"]["created_at"],
        "last_login": login["user"]["last_login"],
        "last_name_change": None,
        "enabled": True,
        "admin": False,
        "password": True,
        "mfa_enabled": False,
        "description": "",
        "tags": [],
        "newsletter": False,
    },
    "session": {
        "id": login["session"]["id"],
        "user_id": login["user"]["id"],
        "device_name": c.headers["User-Agent"],
        "last_update": login["session"]["last_update"],
    },
    "access_token": login["access_token"],
    "refresh_token": login["refresh_token"],
}
assert abs(time.time() - login["user"]["created_at"]) < 2
assert abs(time.time() - login["user"]["last_login"]) < 2
assert abs(time.time() - login["session"]["last_update"]) < 2

resp = c.post(
    "/auth/users",
    json={"name": "user", "display_name": "x", "email": "x@x", "password": "x", "recaptcha_response": "success-1.0"},
)
assert resp.status_code == 409
assert resp.json() == {"detail": "User already exists"}

resp = c.post(
    "/auth/users",
    json={
        "name": "x",
        "display_name": "x",
        "email": "user@example.com",
        "password": "x",
        "recaptcha_response": "success-1.0",
    },
)
assert resp.status_code == 409
assert resp.json() == {"detail": "Email already exists"}

save_auth(login)
user = login["user"]

# get self
for id in ["me", "self", login["user"]["id"]]:
    resp = c.get(f"/auth/users/{id}")
    assert resp.status_code == 200
    assert resp.json() == user

resp = c.get(f"/auth/users/14b871aa-6324-4e41-85ab-1e7fdb0481cb")
assert resp.status_code == 403
assert resp.json() == {"detail": "Permission denied"}

# verify email
resp = c.post("/auth/users/me/email")
assert resp.status_code == 200
assert resp.json() is True

mail = fetch_mail()
assert mail["X-Original-To"] == "user@example.com"
assert mail["Subject"] == "Willkommen bei der Bootstrap Academy!"
content = decode_mail_payload(mail)
code = re.search(r"([A-Z0-9]{4}-){3}[A-Z0-9]{4}", content)
assert code, "Failed to find verification code in email"

resp = c.put("/auth/users/me/email", json={"code": code[0]})
assert resp.status_code == 200
assert resp.json() is True

assert_access_token_invalid()
login = refresh_session()
user["email_verified"] = True
assert login["user"] == user
assert c.get("/auth/users/me").json() == user

resp = c.post(f"/auth/users/14b871aa-6324-4e41-85ab-1e7fdb0481cb/email")
assert resp.status_code == 403
assert resp.json() == {"detail": "Permission denied"}

# update self
## profile
resp = c.patch(
    "/auth/users/me",
    json={"display_name": "321 User", "description": "just a test account", "tags": ["foo", "bar", "test"]},
)
assert resp.status_code == 200
user = resp.json()
user["display_name"] = "321 User"
user["description"] = "just a test account"
user["tags"] = ["foo", "bar", "test"]
assert resp.json() == user
assert c.get("/auth/users/me").json() == user

## name
resp = c.patch("/auth/users/me", json={"name": "test"})
assert resp.status_code == 200
resp = resp.json()
user["name"] = "test"
user["last_name_change"] = resp["last_name_change"]
assert abs(time.time() - resp["last_name_change"]) < 2
assert resp == user
assert c.get("/auth/users/me").json() == user

## email
resp = c.patch("/auth/users/me", json={"email": "other@email"})
assert resp.status_code == 200
user["email"] = "other@email"
user["email_verified"] = False
assert resp.json() == user
assert_access_token_invalid()
login = refresh_session()
assert c.get("/auth/users/me").json() == user

## password
resp = c.patch("/auth/users/me", json={"password": ""})
assert resp.status_code == 403
assert resp.json() == {"detail": "Cannot delete last login method"}

new_password = "otherpw"
resp = c.patch("/auth/users/me", json={"password": new_password})
assert resp.status_code == 200
assert resp.json() == user

discard_auth()
resp = c.post("/auth/sessions", json={"name_or_email": user["name"], "password": password})
assert resp.status_code == 401
assert resp.json() == {"detail": "Invalid credentials"}

password = new_password
resp = c.post("/auth/sessions", json={"name_or_email": user["name"], "password": password})
assert resp.status_code == 201
login = resp.json()
save_auth(login)

user["last_login"] = login["user"]["last_login"]
assert abs(time.time() - user["last_login"]) < 2
assert login["user"] == user

## newsletter
resp = c.patch("/auth/users/me", json={"newsletter": True})
assert resp.status_code == 200
assert resp.json() == user
assert user["newsletter"] is False

mail = fetch_mail()
assert mail[f"X-Original-To"] == user["email"]
assert mail["Subject"] == "Newsletter abonnieren - Bootstrap Academy"
content = decode_mail_payload(mail)
code = re.search(r"([A-Z0-9]{4}-){3}[A-Z0-9]{4}", content)
assert code, "Failed to find verification code in email"

resp = c.put("/auth/users/me/newsletter", json={"code": code[0]})
assert resp.status_code == 200
user["newsletter"] = True
assert resp.json() == user
assert c.get("/auth/users/me").json() == user

resp = c.patch("/auth/users/me", json={"newsletter": False})
assert resp.status_code == 200
user["newsletter"] = False
assert resp.json() == user
assert c.get("/auth/users/me").json() == user

## name (rate limit)
resp = c.patch("/auth/users/me", json={"name": "asdf"})
assert resp.status_code == 403
assert resp.json() == {"detail": "Permission denied"}

## email_verified
resp = c.patch("/auth/users/me", json={"email_verified": True})
assert resp.status_code == 403
assert resp.json() == {"detail": "Permission denied"}

## enabled
resp = c.patch("/auth/users/me", json={"enabled": False})
assert resp.status_code == 403
assert resp.json() == {"detail": "Permission denied"}

## admin
resp = c.patch("/auth/users/me", json={"admin": True})
assert resp.status_code == 403
assert resp.json() == {"detail": "Permission denied"}

assert c.get("/auth/users/me").json() == user

## other user
resp = c.patch(f"/auth/users/14b871aa-6324-4e41-85ab-1e7fdb0481cb", json={"display_name": "foo"})
assert resp.status_code == 403
assert resp.json() == {"detail": "Permission denied"}

# reset password
discard_auth()

## recaptcha error
resp = c.post("/auth/password_reset", json={"email": user["email"], "recaptcha_response": "success-0.3"})
assert resp.status_code == 412
assert resp.json() == {"detail": "Recaptcha failed"}

## success
resp = c.post("/auth/password_reset", json={"email": user["email"], "recaptcha_response": "success-0.7"})
assert resp.status_code == 200
assert resp.json() is True

mail = fetch_mail()
assert mail["X-Original-To"] == user["email"]
assert decode_mail_header(mail["Subject"]) == "Passwort zurücksetzen - Bootstrap Academy"
content = decode_mail_payload(mail)
code = re.search(r"([A-Z0-9]{4}-){3}[A-Z0-9]{4}", content)
assert code, "Failed to find verification code in email"

password = "my new password"
resp = c.put("/auth/password_reset", json={"email": user["email"], "code": code[0], "password": password})
assert resp.status_code == 200
assert resp.json() == user

resp = c.post("/auth/sessions", json={"name_or_email": user["name"], "password": password})
assert resp.status_code == 201
login = resp.json()
save_auth(login)

user["last_login"] = login["user"]["last_login"]
assert abs(time.time() - user["last_login"]) < 2
assert login["user"] == user

# delete self
resp = c.delete("/auth/users/14b871aa-6324-4e41-85ab-1e7fdb0481cb")
assert resp.status_code == 403
assert resp.json() == {"detail": "Permission denied"}

resp = c.delete("/auth/users/me")
assert resp.status_code == 200
assert resp.json() is True
assert_access_token_invalid()
discard_auth()

resp = c.post("/auth/sessions", json={"name_or_email": user["name"], "password": password})
assert resp.status_code == 401
assert resp.json() == {"detail": "Invalid credentials"}

# admin: create via cli
status, _ = subprocess.getstatusoutput(
    "academy admin user create --admin --verified admin admin@example.com supersecureadminpassword"
)
assert status == 0

resp = c.post("/auth/sessions", json={"name_or_email": "admin", "password": "supersecureadminpassword"})
assert resp.status_code == 201
login = resp.json()
save_auth(login)
assert login["user"]["admin"] is True

assert subprocess.getstatusoutput("academy admin user create a a@a a")[0] == 0
assert subprocess.getstatusoutput("academy admin user create b b@b b")[0] == 0
assert subprocess.getstatusoutput("academy admin user create c c@c c")[0] == 0
assert subprocess.getstatusoutput("academy admin user create d d@d d")[0] == 0

# admin: list users
resp = c.get("/auth/users")
assert resp.status_code == 200
resp = resp.json()
assert resp["total"] == 5
assert len(resp["users"]) == 5
assert resp["users"][0] == login["user"]
assert all(a["name"] == b for a, b in zip(resp["users"], ["admin", "a", "b", "c", "d"]))
a = resp["users"][1]

# admin: get other
resp = c.get(f"/auth/users/{a['id']}")
assert resp.status_code == 200
resp = resp.json()
assert resp == a

# admin: update other
resp = c.patch(
    f"/auth/users/{a['id']}",
    json={"name": "foo", "display_name": "foo", "email_verified": True, "admin": True, "enabled": False},
)
assert resp.status_code == 200
a["name"] = "foo"
a["display_name"] = "foo"
a["email_verified"] = True
a["admin"] = True
a["enabled"] = False
assert resp.json() == a
assert c.get(f"/auth/users/{a['id']}").json() == a

# admin: delete other
resp = c.delete(f"/auth/users/{a['id']}")
assert resp.status_code == 200
assert resp.json() is True

resp = c.get(f"/auth/users/{a['id']}")
assert resp.status_code == 404
assert resp.json() == {"detail": "User not found"}
