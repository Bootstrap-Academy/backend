import re
import subprocess

from utils import c, create_verified_account, decode_mail_header, decode_mail_payload, fetch_mail

login = create_verified_account("a", "a@a", "a")

# list courses
resp = c.get("/skills/courses")
assert resp.status_code == 200
courses = resp.json()
assert isinstance(courses, list)
assert all(isinstance(c, dict) for c in courses)
assert all(f in c for c in courses for f in ["id", "title", "description", "authors", "last_update", "sections"])

unfree = next(c for c in courses if c["price"] != 0)
free = next(c for c in courses if c["price"] == 0)

# purchase course

## not found
resp = c.post("/skills/course_access/does_not_exist")
assert resp.status_code == 404
assert resp.json() == {"detail": "Course not found"}

## course is free
resp = c.post(f"/skills/course_access/{free["id"]}")
assert resp.status_code == 403
assert resp.json() == {"detail": "Course is free"}

## not enough coins
resp = c.post(f"/skills/course_access/{unfree["id"]}")
assert resp.status_code == 412
assert resp.json() == {"detail": "Not enough coins"}

## ok
assert subprocess.getstatusoutput(f"academy admin coin add {login['user']['id']} {unfree["price"] + 7}")[0] == 0
resp = c.post(f"/skills/course_access/{unfree["id"]}")
assert resp.status_code == 200
assert resp.json() is True
assert c.get("/shop/coins/me").json()["coins"] == 7

mail = fetch_mail()
assert mail["X-Original-To"] == "a@a"
assert decode_mail_header(mail["Subject"]) == "Kaufbestätigung - Bootstrap Academy"
content = decode_mail_payload(mail)
assert f'Danke für den Kauf des Kurses "{unfree["title"]}"!' in content

## already purchased
resp = c.post(f"/skills/course_access/{unfree["id"]}")
assert resp.status_code == 403
assert resp.json() == {"detail": "Already purchased course"}
