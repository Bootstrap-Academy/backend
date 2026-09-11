import email
import email.header
import os
import subprocess
import time
from email.message import Message
from pathlib import Path
from typing import cast

import httpx
import pyotp


def fetch_mail() -> Message:
    t = time.time()
    p = Path("/var/mail/root/new")
    mail = None
    while (not p.is_dir() or not ((mail := next(p.iterdir(), None)))) and time.time() - t < 20:
        time.sleep(1)
    assert mail, "No email received"
    msg = email.message_from_bytes(mail.read_bytes())
    mail.unlink()
    return msg


def decode_mail_header(header):
    return str(email.header.make_header(email.header.decode_header(header)))


def get_mail_parts(mail: Message) -> list[Message]:
    return cast(list[Message], mail.get_payload())


def decode_mail_part(mail: Message) -> bytes:
    return cast(bytes, mail.get_payload(decode=True))


def decode_mail_payload(mail: Message):
    return decode_mail_part(get_mail_parts(mail)[0]).decode()


def refresh_session(refresh_token=None, client=None):
    client = client or c
    refresh_token = refresh_token or getattr(client, "_refresh_token")
    resp = client.put("/auth/session", json={"refresh_token": refresh_token})
    assert resp.status_code == 200
    login = resp.json()
    save_auth(login, client)
    return login


def assert_access_token_invalid(client=None):
    client = client or c
    resp = client.get("/auth/users/me")
    assert resp.status_code == 401
    assert resp.json() == {"detail": "Invalid token"}


def save_auth(login, client=None):
    client = client or c
    client.headers["Authorization"] = f"Bearer {login['access_token']}"
    setattr(client, "_refresh_token", login["refresh_token"])


def discard_auth(client=None):
    client = client or c
    client.headers.pop("Authorization", None)


def make_client():
    return httpx.Client(base_url="http://127.0.0.1:8000", timeout=httpx.Timeout(60))


def make_internal_client(aud):
    client = make_client()

    def auth(req):
        status, jwt = subprocess.getstatusoutput(f'academy jwt sign \'{{"aud":"{aud}"}}\'')
        assert status == 0
        req.headers["Authorization"] = jwt.strip()
        return req

    client.auth = auth
    return client


def create_account(name, email, password, client=None):
    client = client or c
    resp = client.post(
        "/auth/users",
        json={
            "name": name,
            "display_name": name,
            "email": email,
            "password": password,
            "terms_version": "2026-09-r2",
            "age_confirmed": True,
            "recaptcha_response": "success-1.0",
        },
    )
    assert resp.status_code == 200
    login = resp.json()
    save_auth(login, client)
    return login


def create_verified_account(name, email, password, client=None):
    client = client or c
    os.system(f"academy admin user create --verified {name} {email} {password}")
    resp = client.post("/auth/sessions", json={"name_or_email": name, "password": password})
    assert resp.status_code == 200
    login = resp.json()
    assert login["user"]["email_verified"] is True
    save_auth(login, client)
    return login


def wait_for_new_totp_window():
    """Wait until the current TOTP code changes.

    A code that has just been used is rejected as a replay for the rest of its
    window.
    """
    time.sleep(31 - time.time() % 30)


def setup_mfa(client=None):
    """Enable TOTP for the authenticated account and return the authenticator."""
    client = client or c
    resp = client.post("/auth/users/me/mfa")
    assert resp.status_code == 200
    totp = pyotp.TOTP(resp.json())

    resp = client.put("/auth/users/me/mfa", json={"code": totp.now()})
    assert resp.status_code == 200
    return totp


def create_admin_account(name, email, password, client=None):
    """Create an administrator and log in with a second factor.

    Administrative endpoints require a session that was authenticated with
    TOTP, so the account gets an authenticator before the session is created.
    """
    client = client or c
    os.system(f"academy admin user create --admin --verified {name} {email} {password}")
    resp = client.post("/auth/sessions", json={"name_or_email": name, "password": password})
    assert resp.status_code == 200
    save_auth(resp.json(), client)

    totp = setup_mfa(client)
    wait_for_new_totp_window()

    resp = client.post("/auth/sessions", json={"name_or_email": name, "password": password, "mfa_code": totp.now()})
    assert resp.status_code == 200
    login = resp.json()
    assert login["user"]["email_verified"] is True
    assert login["user"]["admin"] is True
    assert login["user"]["mfa_enabled"] is True
    assert login["session"]["mfa_verified"] is True
    save_auth(login, client)
    return login


def get_self(client=None):
    client = client or c
    resp = client.get("/auth/users/me")
    assert resp.status_code == 200
    return resp.json()


c = make_client()


def enable_premium_renewal(client=None):
    """Explicitly order the current monthly coin renewal; never use legacy autopay."""
    from uuid import uuid4

    client = client or c
    offer = client.get("/shop/premium/renewal-offer")
    assert offer.status_code == 200, offer.text
    response = client.put(
        "/shop/premium/autopay",
        json={
            "plan": "MONTHLY",
            "consent": {
                "request_id": str(uuid4()),
                "offer_id": offer.json()["id"],
                "accepted": True,
                "withdrawal_consent": True,
            },
        },
    )
    assert response.status_code == 200, response.text
    status = client.get("/shop/premium/me").json()
    assert status["autopay"] == "MONTHLY"
    assert status["renewal"]["confirmation_sent"] is True
    return status


def purchase_offer(kind, client=None):
    client = client or c
    response = client.post(f"/shop/purchases/offers/{kind}")
    assert response.status_code == 200, response.text
    status = response.json()
    assert status["state"] == "offered", status
    return status


def purchase_acceptance(status):
    return {
        "order_id": status["offer"]["id"],
        "offer_hash": status["offer"]["hash"],
        "accepted": True,
        "early_performance_requested": True,
    }


def purchase(kind, client=None):
    """One explicit offer/acceptance; recover only this order, never place another."""
    client = client or c
    offered = purchase_offer(kind, client)
    response = client.post("/shop/purchases/accept", json=purchase_acceptance(offered))
    assert response.status_code == 200, response.text
    status = response.json()
    deadline = time.monotonic() + 20
    while status["state"] == "paid" and time.monotonic() < deadline:
        time.sleep(0.2)
        response = client.get(f"/shop/purchases/{offered['offer']['id']}")
        assert response.status_code == 200, response.text
        status = response.json()
    assert status["offer"] == offered["offer"]
    assert status["state"] == "fulfilled", status
    assert status["confirmation_smtp_accepted_at"] is not None
    assert status["fulfillment"] is not None
    return status


def paypal_order(coins, client=None):
    client = client or c
    response = client.post(f"/shop/coins/paypal/offers/{coins}")
    assert response.status_code == 200, response.text
    offered = response.json()
    response = client.post("/shop/coins/paypal/orders", json={"coins": coins, **purchase_acceptance(offered)})
    assert response.status_code == 200, response.text
    return response.json()


def configure_purchases():
    """Explicit synthetic windows in this disposable VM; defaults stay closed."""
    assert Path(__file__).resolve().parent == Path("/root/tests")
    assert subprocess.check_output(["hostname"], text=True).strip() == "machine"
    path = Path("/run/academy-backend/secrets.toml")
    original = path.read_text()
    assert "[purchase.provision_window_seconds]" not in original
    path.write_text(
        original + "\n[purchase.provision_window_seconds]\n"
        "premium_monthly = 60\npremium_yearly = 60\nhearts = 60\ncoins = 60\n"
    )
    subprocess.run(["systemctl", "restart", "academy-backend.service"], check=True)
    for _ in range(100):
        try:
            response = c.get("/health")
            if response.status_code == 200:
                return
        except httpx.TransportError:
            pass
        time.sleep(0.1)
    raise AssertionError("Disposable VM backend did not restart with explicit test windows")
