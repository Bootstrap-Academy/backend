# Public feedback

The guest endpoint `POST /feedback` publishes issues only to
`Bootstrap-Academy/Bootstrap-Academy`. It does not use a session, user ID or
contact email. The existing private contact endpoint remains independent.

## Configuration

Feedback is disabled by default. To enable it:

```toml
[feedback]
enabled = true
github_token_file = "/run/secrets/feedback-github-token"
storage_path = "/var/lib/academy/feedback"
public_base_url = "https://api.example.org"
```

The token file must be readable only by the service/operator. Use a dedicated
GitHub token with access to the fixed repository and issue read/write permission.
There is no client-supplied repository or upstream URL. The token is never
included in a response, journal, public configuration or log. Rotation takes
effect when the backend restarts. Invalid enabled configuration stops startup.

`storage_path` must be a persistent directory owned by the service with private
permissions (0700). Exactly one process may own this path; an OS file lock
rejects concurrent writers. Do not use this path with multiple replicas or an
unreliable network filesystem. Include the receipts in backup/restore plans;
restoring an older journal without reconciling reports can lose idempotency.

## Request contract

```json
{
  "request_id": "41907782-cefa-4bca-a918-794b736317d6",
  "kind": "bug",
  "title": "The selected action does not open",
  "description": "What happened and how to reproduce it",
  "diagnostics_consent": false
}
```

`request_id` is a random UUID v4 created once for the final draft; retain the
same UUID and exact payload after an ambiguous failure. `kind` is `bug` or
`feature`. Title is nonempty, one line, at most 256 characters; description is
nonempty and at most 4096 characters. All objects reject unknown keys.

The optional `diagnostics` object is accepted only when
`diagnostics_consent: true`; true also requires the object. Its required fields
are `app_build` (128 bytes), `browser` (80), `os` (80), `viewport` (32),
`language` (32), `theme` (`light`, `dark`, `system`) and `reduced_motion` (bool).
Optional `area` is `home`, `learning`, `courses`, `challenges`, `profile`,
`account`, `settings`, `shop`, `events` or `other`; optional `error_code` is
limited to 64 bytes. Technical strings accept ASCII letters/digits, space,
`.`, `_`, `-`, `(`, `)` and `×`. Unknown values use `unknown`. Raw URLs,
account paths, headers, console, storage and arbitrary debug fields are rejected.

An independent optional screenshot has exactly `{ "data_url": "data:image/png;base64,..." }`
or the JPEG MIME equivalent. The final image is decoded and re-encoded as PNG;
source metadata and trailing data are discarded. Transparency is flattened
onto white so invisible RGB channels cannot retain concealed pixels. Input and output are each
limited to 3 MiB, each dimension to 4096 pixels and total pixels to 8,388,608.
The complete JSON body is limited to 5 MiB. No source image is persisted.

## Results and uncertain delivery

- `200 {"status":"created","issue_url":"https://github.com/Bootstrap-Academy/Bootstrap-Academy/issues/123"}`
  means GitHub creation was verified and its URL was durably recorded.
- `202 {"status":"pending","request_id":"..."}` means publication is
  uncertain. An explicit retry with the exact same draft performs reconciliation
  only. It never creates another issue.
- Error bodies contain `error` and `message`. Invalid requests use 400/413/422;
  differing content under a used UUID uses 409 `request_conflict`; rate limits
  use 429 `rate_limited` with `Retry-After`; unavailable service or storage uses
  503 `unavailable` / `capacity_exceeded`.

Do not turn a network error or proxy 5xx into a new UUID. The upstream may have
accepted the original even when its response was lost. An unchanged request
with a saved successful receipt always returns the original issue link.

Before the first GitHub POST, an atomic file write and directory fsync record
the UUID, canonical payload SHA-256, private random reconciliation marker,
creation time, optional public image identifier and initially absent issue URL.
No title, description, diagnostic values, IP address or source image are kept
in the receipt. HTTP redirects and library retries are disabled. On retry,
the backend lists the fixed repository (including closed issues) and verifies
the exact server marker and fixed-repository issue URL. At most 1000 issues
and 20 seconds are examined per reconciliation, at most once per minute per
request. Empty or failed lookup remains pending, never permission to repost.

This intentionally favors avoiding duplicate publication. A crash after the
receipt fsync but before the POST, denied GitHub credentials, a removed marker
or an issue outside the bounded scan can leave a request pending. There is no
automatic resend worker. Operators must inspect the receipt marker against
GitHub and resolve the ambiguity before any manual replacement publication;
never delete a pending receipt merely to force a retry. No diagnostic payload
or authentication token should be pasted into an operational log.

## Storage, lifecycle and operational limits

Only re-encoded images are served by `GET /feedback/images/{random_uuid}` with
`image/png`, `nosniff`, a restrictive CSP and `Cache-Control: no-store`. The
request UUID does not reveal the separate random image ID. Images are publicly
accessible by that URL; do not place private material in them. Local images
become unavailable exactly 90 days after acceptance and are removed at startup
and at least hourly while running. The issue states this lifetime. GitHub or
other recipients may retain their own copies; local expiry does not promise
erasure of external copies.

An operator can remove a specific image immediately by deleting its random
UUID `.png` file from `images/`; the endpoint then returns 404. Preserve the
receipt to avoid duplicate issues. Public text must be edited/removed on GitHub
through the authorized moderation/contact process. Removing a GitHub issue
alone does not remove the locally hosted screenshot.

The service admits at most two simultaneous uploads and one publishing or
reconciliation operation. POST admission is limited to 30 per client IP/hour
and 120 globally/hour; it uses the existing trusted-proxy client-IP middleware.
Counters contain only IP and timestamps in bounded process memory for at most
an hour and are not sent to GitHub. Persistent receipts additionally limit
new submissions to 100 per rolling day across restarts. Client-IP counters
reset at process restart; the durable global creation limit remains.

Image storage is capped at 256 MiB and receipts at 10,000 (each under 4 KiB).
New requests fail before publication when capacity is reached. Minimal
idempotency receipts have no automatic expiry; they are capped and retained
while this feedback service accepts request IDs. Monitor capacity and perform
an explicit migration/retention decision before changing that protocol; do not
silently purge receipts and thereby allow old UUIDs to publish twice. Orphaned
images and interrupted temporary writes are removed on recovery/cleanup.

## Verification

`cargo test -p academy_api_rest feedback --lib` uses only a loopback fake GitHub
server. It covers guests/text minimization, consent and key rejection,
payload conflicts, restart recovery after a lost creation response, absent
lookup without duplicates, image sanitization/expiry, upload/rate/storage
limits and single-writer ownership. No public issue is created by those tests.
