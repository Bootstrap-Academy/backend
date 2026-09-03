from urllib.parse import parse_qs, urlparse

from utils import c, create_account, discard_auth, get_self, save_auth

REDIRECT_URI = "http://localhost/oauth2/callback"


def begin_authorization(provider_id="test"):
    """Ask the backend for an authorize URL and return it with its state."""
    resp = c.post("/auth/oauth/authorize", json={"provider_id": provider_id, "redirect_uri": REDIRECT_URI})
    assert resp.status_code == 200
    body = resp.json()
    return body["authorize_url"], body["state"]


def authenticate(id, name):
    """Run a full authorization flow and return the callback to submit."""
    authorize_url, state = begin_authorization()

    query = parse_qs(urlparse(authorize_url).query)
    assert query["response_type"] == ["code"]
    assert query["client_id"] == ["client-id"]
    assert query["redirect_uri"] == [REDIRECT_URI]
    assert query["state"] == [state]
    # the state is unguessable, not the provider id
    assert len(state) == 64
    assert state != "test"
    # PKCE is enabled for this provider
    assert query["code_challenge_method"] == ["S256"]
    assert len(query["code_challenge"][0]) == 43

    resp = c.post(authorize_url, data={"id": str(id), "name": name}, follow_redirects=False)
    assert resp.is_redirect
    url = urlparse(resp.headers["location"])
    query = parse_qs(url.query)
    assert query["state"] == [state]
    return {"state": state, "code": query["code"][0]}


resp = c.get("/auth/oauth/providers")
assert resp.status_code == 200
assert resp.json() == [{"id": "test", "name": "Test OAuth2 Provider"}]

# an authorization flow can only be started for a known provider
resp = c.post("/auth/oauth/authorize", json={"provider_id": "does-not-exist", "redirect_uri": REDIRECT_URI})
assert resp.status_code == 404
assert resp.json() == {"detail": "Provider not found"}

# two flows get two different states
_, state_a = begin_authorization()
_, state_b = begin_authorization()
assert state_a != state_b

# a state that was never issued is rejected
resp = c.post("/auth/sessions/oauth", json={"state": "x" * 64, "code": "somecode"})
assert resp.status_code == 401
assert resp.json() == {"detail": "Invalid state"}

# create link
login = create_account("a", "a@a", "a")
user = login["user"]
callback = authenticate(42, "foo")
resp = c.post("/auth/oauth/links/me", json=callback)
assert resp.status_code == 200
link = resp.json()
assert link == {"id": link["id"], "provider_id": "test", "display_name": "foo"}

# the state of that flow cannot be redeemed a second time
resp = c.post("/auth/oauth/links/me", json=callback)
assert resp.status_code == 401
assert resp.json() == {"detail": "Invalid state"}

# list links
resp = c.get("/auth/oauth/links/me")
assert resp.status_code == 200
assert resp.json() == [link]

# login
discard_auth()
resp = c.post("/auth/sessions/oauth", json=authenticate(42, "foo"))
assert resp.status_code == 200
login = resp.json()["login"]
user["last_login"] = login["user"]["last_login"]
assert login["user"] == user
save_auth(login)

# remove password
resp = c.patch("/auth/users/me", json={"password": ""})
assert resp.status_code == 200
user["password"] = False
assert resp.json() == user
assert get_self() == user

resp = c.delete(f"/auth/oauth/links/me/{link['id']}")
assert resp.status_code == 403
assert resp.json() == {"detail": "Cannot delete last login method"}

resp = c.patch("/auth/users/me", json={"password": "a"})
assert resp.status_code == 200
user["password"] = True
assert resp.json() == user
assert get_self() == user

# delete link
resp = c.delete(f"/auth/oauth/links/me/{link['id']}")
assert resp.status_code == 200
assert resp.json() is True

resp = c.get("/auth/oauth/links/me")
assert resp.status_code == 200
assert resp.json() == []

# register
discard_auth()
resp = c.post("/auth/sessions/oauth", json=authenticate(43, "bar"))
assert resp.status_code == 200
register_token = resp.json()["register_token"]

signup = {
    "name": "b",
    "display_name": "b",
    "email": "b@b",
    "oauth_register_token": register_token,
    "terms_version": "2026-09",
    "age_confirmed": True,
    "recaptcha_response": "success-1.0",
}
resp = c.post("/auth/users", json=signup)
assert resp.status_code == 200
login = resp.json()
user = login["user"]
save_auth(login)
assert get_self() == user
assert user["password"] is False

# the registration token is single use
discard_auth()
resp = c.post("/auth/users", json={**signup, "name": "c", "email": "c@c"})
assert resp.status_code == 401
assert resp.json() == {"detail": "Invalid OAuth token"}
save_auth(login)

resp = c.get("/auth/oauth/links/me")
assert resp.status_code == 200
links = resp.json()
assert links == [{"id": links[0]["id"], "provider_id": "test", "display_name": "bar"}]

discard_auth()
resp = c.post("/auth/sessions/oauth", json=authenticate(43, "bar"))
assert resp.status_code == 200
login = resp.json()["login"]
user["last_login"] = login["user"]["last_login"]
assert login["user"] == user
save_auth(login)
