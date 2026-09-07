import os

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
os.system("academy admin user create --disabled b b@b b")
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
