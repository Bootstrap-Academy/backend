from utils import c, decode_mail_payload, fetch_mail

msg = {
    "name": "Some User",
    "email": "some.user@example.com",
    "subject": "Something Important",
    "message": "This is a really important message.",
}

resp = c.post("/auth/contact", json={**msg, "recaptcha_response": "success-0.7"})
assert resp.status_code == 200
assert resp.json() is True

mail = fetch_mail()
assert mail["X-Original-To"] == "contact@academy"
assert mail["Subject"] == "[Contact Form] Something Important"
content = decode_mail_payload(mail)
assert content == "Message from Some User (some.user@example.com):\n\nThis is a really important message."

for resp in ["success-0.3", "failure"]:
    resp = c.post("/auth/contact", json={**msg, "recaptcha_response": resp})
    assert resp.status_code == 412
    assert resp.json() == {"detail": "Recaptcha failed"}
