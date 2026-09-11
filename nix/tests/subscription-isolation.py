"""Subscription writes through the public API and renewal task stay user-scoped."""

import subprocess

from utils import create_verified_account, make_client, enable_premium_renewal


def coin_add(user_id, coins):
    subprocess.run(["academy", "admin", "coin", "add", user_id, "--", str(coins)], check=True)


def expire_premium(user_id):
    # These are generated test-account UUIDs in the isolated VM database. Expire
    # only the selected account, without changing the clock or other accounts.
    subprocess.run(
        [
            "sudo",
            "-u",
            "postgres",
            "psql",
            "-d",
            "academy",
            "-v",
            "ON_ERROR_STOP=1",
            "-c",
            f"UPDATE premium SET since = NOW() - INTERVAL '2 months', "
            f"until = NOW() - INTERVAL '1 month' WHERE user_id = '{user_id}'; "
            f"UPDATE premium_renewal_delivery d SET sent_at=NOW()-INTERVAL '2 months' "
            f"FROM premium_renewal_agreements a WHERE a.id=d.agreement_id AND a.user_id='{user_id}' AND d.sent_at IS NOT NULL",
        ],
        check=True,
    )


def status(client):
    response = client.get("/shop/premium/me")
    assert response.status_code == 200
    return response.json()


def cancel(email, agreement=None):
    # No bearer token: this is the anonymous statutory declaration path.
    response = anonymous.post(
        "/contracts/cancellations",
        json={
            "name": "Test Subscriber",
            "email": email,
            "contract": "PREMIUM",
            "cancellation_type": "ORDINARY",
            "renewal_agreement_id": agreement,
        },
    )
    assert response.status_code == 200, response.text


first, other, non_subscriber, anonymous = (make_client() for _ in range(4))
first_login = create_verified_account("first", "first@example.com", "password", first)
other_login = create_verified_account("other", "other@example.com", "password", other)
create_verified_account("free", "free@example.com", "password", non_subscriber)

for client, login in [(first, first_login), (other, other_login)]:
    coin_add(login["user"]["id"], 50000)
    response = client.post(
        "/shop/premium",
        json={"plan": "MONTHLY", "autopay": False, "withdrawal_consent": True, "withdrawal_text_version": "2026-09"},
    )
    assert response.status_code == 200
    assert response.json()["autopay"] is None
    enable_premium_renewal(client)

first_paid = status(first)
other_paid = status(other)
assert first_paid["autopay"] == other_paid["autopay"] == "MONTHLY"
other_coins = other.get("/shop/coins/me").json()

cancel("free@example.com")
assert status(first) == first_paid
assert status(other) == other_paid

for _ in range(2):
    cancel("first@example.com", first_paid["renewal"]["id"])
    assert status(first) == {**first_paid, "autopay": None, "renewal": None}
    assert status(other) == other_paid

# Fresh consent and rejected unsupported yearly writes preserve the other user.
for _ in range(2):
    enable_premium_renewal(first)
    assert status(other) == other_paid
response = first.put("/shop/premium/autopay", json={"plan": "YEARLY"})
assert response.status_code == 412
assert status(first)["autopay"] == "MONTHLY"
assert status(other) == other_paid

expire_premium(first_login["user"]["id"])
subprocess.run(["systemctl", "start", "--wait", "academy-task-refresh-premium.service"], check=True)
assert status(first)["premium"] is True
assert status(other) == other_paid
assert other.get("/shop/coins/me").json() == other_coins

# Failed renewal cancels just the account that cannot pay.
remaining = first.get("/shop/coins/me").json()["coins"]
coin_add(first_login["user"]["id"], -remaining)
expire_premium(first_login["user"]["id"])
subprocess.run(["systemctl", "start", "--wait", "academy-task-refresh-premium.service"], check=True)
assert status(first)["autopay"] is None
assert status(first)["premium"] is False
assert status(other) == other_paid
assert other.get("/shop/coins/me").json() == other_coins
