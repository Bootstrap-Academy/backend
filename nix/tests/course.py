from utils import c

# list courses
resp = c.get("/skills/courses")
assert resp.status_code == 200
courses = resp.json()
assert isinstance(courses, list)
assert all(isinstance(c, dict) for c in courses)
assert all(f in c for c in courses for f in ["id", "title", "description", "authors", "last_update", "sections"])
