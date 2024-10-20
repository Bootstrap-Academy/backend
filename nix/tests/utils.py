import email
import email.header
import os
import time
from email.message import Message
from pathlib import Path
from typing import cast

import httpx


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


def decode_mail_payload(mail: Message):
    return cast(bytes, mail.get_payload(decode=True)).decode()


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


def create_account(name, email, password, client=None):
    client = client or c
    resp = client.post(
        "/auth/users",
        json={
            "name": name,
            "display_name": name,
            "email": email,
            "password": password,
            "recaptcha_response": "success-1.0",
        },
    )
    assert resp.status_code == 200
    login = resp.json()
    save_auth(login, client)
    return login


def create_admin_account(name, email, password, client=None):
    client = client or c
    os.system(f"academy admin user create --admin {name} {email} {password}")
    resp = client.post("/auth/sessions", json={"name_or_email": name, "password": password})
    assert resp.status_code == 200
    login = resp.json()
    save_auth(login, client)
    return login


def get_self(client=None):
    client = client or c
    resp = client.get("/auth/users/me")
    assert resp.status_code == 200
    return resp.json()


c = make_client()
