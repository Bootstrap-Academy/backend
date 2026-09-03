from urllib.parse import parse_qs, urlparse

from utils import c, create_account, discard_auth, get_self, save_auth


def authenticate(id, name):
    resp = c.post(
        "http://127.0.0.1:8101/oauth2/authorize?response_type=code&client_id=client-id&state=test123&redirect_uri=http://localhost/oauth2/callback",
        data={"id": str(id), "name": name},
        follow_redirects=False,
    )
    assert resp.is_redirect
    url = urlparse(resp.headers["location"])
    query = parse_qs(url.query)
    code = query["code"][0]
    state = query["state"][0]
    assert state == "test123"
    return {"provider_id": "test", "code": code, "redirect_uri": "http://localhost/oauth2/callback"}


resp = c.get("/auth/oauth/providers")
assert resp.status_code == 200
assert resp.json() == [
    {
        "id": "test",
        "name": "Test OAuth2 Provider",
        "authorize_url": "http://127.0.0.1:8101/oauth2/authorize?response_type=code&client_id=client-id",
    }
]

# create link
login = create_account("a", "a@a", "a")
user = login["user"]
resp = c.post("/auth/oauth/links/me", json=authenticate(42, "foo"))
assert resp.status_code == 200
link = resp.json()
assert link == {"id": link["id"], "provider_id": "test", "display_name": "foo"}

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

resp = c.post(
    "/auth/users",
    json={
        "name": "b",
        "display_name": "b",
        "email": "b@b",
        "oauth_register_token": register_token,
        "terms_version": "2026-09",
        "age_confirmed": True,
        "recaptcha_response": "success-1.0",
    },
)
assert resp.status_code == 200
login = resp.json()
user = login["user"]
save_auth(login)
assert get_self() == user
assert user["password"] is False

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
