import os

import pyotp
from utils import (
    assert_access_token_invalid,
    c,
    create_account,
    discard_auth,
    get_self,
    refresh_session,
    save_auth,
)

login = create_account("a", "a@a", "a")
assert login["user"]["mfa_enabled"] is False
assert get_self()["mfa_enabled"] is False

resp = c.post("/auth/users/me/mfa")
assert resp.status_code == 200
totp = pyotp.TOTP(resp.json())

assert get_self()["mfa_enabled"] is False

resp = c.put("/auth/users/me/mfa", json={"code": totp.now()})
assert resp.status_code == 200
recovery_code = resp.json()

assert get_self()["mfa_enabled"] is True

resp = c.delete("/auth/users/me/mfa")
assert resp.status_code == 200
assert resp.json() is True

# Removing the second factor ends the authority it granted: `mfa_verified` is
# cleared on every session of the account and the access tokens that carry it
# are invalidated, so the session has to be refreshed before it can be used
# again.
assert_access_token_invalid()
login = refresh_session()
assert login["session"]["mfa_verified"] is False

assert get_self()["mfa_enabled"] is False

# not initialized
resp = c.put("/auth/users/me/mfa", json={"code": totp.now()})
assert resp.status_code == 412
assert resp.json() == {"detail": "MFA not initialized"}

resp = c.delete("/auth/users/me/mfa")
assert resp.status_code == 412
assert resp.json() == {"detail": "MFA not enabled"}

# not enabled
resp = c.post("/auth/users/me/mfa")
assert resp.status_code == 200

resp = c.post("/auth/users/me/mfa")
assert resp.status_code == 200
totp = pyotp.TOTP(resp.json())

resp = c.delete("/auth/users/me/mfa")
assert resp.status_code == 412
assert resp.json() == {"detail": "MFA not enabled"}

# already enabled
resp = c.put("/auth/users/me/mfa", json={"code": totp.now()})
assert resp.status_code == 200
recovery_code = resp.json()

resp = c.post("/auth/users/me/mfa")
assert resp.status_code == 409
assert resp.json() == {"detail": "MFA already enabled"}

resp = c.put("/auth/users/me/mfa", json={"code": totp.now()})
assert resp.status_code == 409
assert resp.json() == {"detail": "MFA already enabled"}

# invalid code
resp = c.delete("/auth/users/me/mfa")
assert resp.status_code == 200
assert resp.json() is True
refresh_session()

resp = c.post("/auth/users/me/mfa")
assert resp.status_code == 200
totp = pyotp.TOTP(resp.json())

resp = c.put("/auth/users/me/mfa", json={"code": "293843"})
assert resp.status_code == 412
assert resp.json() == {"detail": "Invalid code"}
assert get_self()["mfa_enabled"] is False

# login
resp = c.put("/auth/users/me/mfa", json={"code": totp.now()})
assert resp.status_code == 200
recovery_code = resp.json()
assert get_self()["mfa_enabled"] is True

discard_auth()
resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "a"})
assert resp.status_code == 412
assert resp.json() == {"detail": "Invalid code"}

os.system("date -s '+30sec'")
code = totp.now()
resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "a", "mfa_code": code})
assert resp.status_code == 200
login = resp.json()
assert login["user"]["mfa_enabled"] is True

# recently used
resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "a", "mfa_code": code})
assert resp.status_code == 412
assert resp.json() == {"detail": "Invalid code"}

# invalid code
discard_auth()
resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "a", "mfa_code": "283842"})
assert resp.status_code == 412
assert resp.json() == {"detail": "Invalid code"}

# invalid recovery code
resp = c.post("/auth/sessions", json={"name_or_email": "a", "password": "a", "recovery_code": recovery_code[::-1]})
assert resp.status_code == 412
assert resp.json() == {"detail": "Invalid code"}

# recovery code
resp = c.post(
    "/auth/sessions",
    json={"name_or_email": "a", "password": "a", "recovery_code": recovery_code, "recaptcha_response": "success-1.0"},
)
assert resp.status_code == 200
login = resp.json()
assert login["user"]["mfa_enabled"] is False
save_auth(login)
assert get_self()["mfa_enabled"] is False
