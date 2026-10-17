# Terms 2026-10: start, PDFs and bundle

AGB and Widerrufsbelehrung 2026-10 take effect at one moment, the start. Everything below uses
that one value: an ISO timestamp with an offset that is 00:00 in Europe/Berlin, for example
`2026-12-09T00:00:00+01:00` in winter or `2027-06-01T00:00:00+02:00` in summer. Write it the
same way everywhere. Frontend, this script and the backend refuse any other moment, so the date
in the texts and the moment of the switch are the same day.

| Where | What the start does |
|---|---|
| Frontend build, `NUXT_PUBLIC_TERMS_2026_10_FROM` | Unset: the 2026-10 pages do not exist. Set: `/docs/terms-and-conditions-2026-10` and `/docs/right-of-withdrawal-2026-10` exist and say until the start that they do not apply yet. From the start the main pages show 2026-10, signup offers 2026-10 and the gate for older accounts no longer asks. The browser decides by its clock, so the switch needs no new build. |
| `scripts/render-purchase-documents.py --release 2026-10` | Renders both PDFs with the date in the text, checks the whole text with `pdftotext` and writes `purchase-document-manifest-2026-10.json` next to them: the start, the terms version, both file names with their SHA-256 and the frontend commit. It prints the SHA-256 of the manifest. |
| Backend `[purchase.stage2]` | `manifest_path` and `manifest_sha256` of that manifest. The backend reads start, version, PDFs and their SHA-256 from it. From the start its own offers and the monthly Premium renewal bind the PDFs of 2026-10. |
| Backend `[user]` | `registration_terms_version = "2026-10"` and `registration_terms_from`, the same start. The backend refuses to start if the two starts differ or one of them is missing. |

## Steps

1. **Test acceptance.** Choose a provisional start for Test only. Build the frontend of Test
   with `NUXT_PUBLIC_TERMS_2026_10_FROM` set to it, render the PDFs (step 3) from exactly the
   deployed frontend commit, and configure the backend of Test as in step 4. A start in the past
   shows the state after the switch, one in the future the announcement.
2. **Announcement.** Mail and notice go out once stage 2 is accepted on Test. The start is at
   least seven weeks after sending (AGB 20.1 asks for six weeks after receipt).
3. **Render** with the final start, from a frontend checkout at the commit that goes live:

   ```sh
   CHROMIUM=/run/current-system/sw/bin/chromium \
   PDFTOTEXT=<poppler-utils>/bin/pdftotext \
   python3 -I -B backend/scripts/render-purchase-documents.py --release 2026-10 \
     --effective-date 2026-12-09T00:00:00+01:00 \
     --frontend <frontend checkout> --output-dir <new directory outside the repository>
   ```

   The script stops if a source block or the whole text is missing from a PDF, if the date is
   not 00:00 in Berlin, or if the output directory is inside this repository or not empty. The
   manifest records the frontend commit; a `-dirty` suffix means the checkout had changes, and
   the backend refuses such a manifest. The text of a render is reproducible, its bytes are
   not: two renders of the same commit give the same text with other SHA-256 values. Render
   once per host, or copy one render to both hosts; either way each host pins the manifest it
   has.
4. **Configuration, before the start.** Copy the whole output directory (both PDFs and the
   manifest) to a directory on the host that survives a reboot and that the backend service
   can read, for example `/persistent/data/academy/legal-2026-10/`. Then set, with the SHA-256
   the script printed and the start of step 3:

   ```toml
   [purchase.stage2]
   manifest_path = "/persistent/data/academy/legal-2026-10/purchase-document-manifest-2026-10.json"
   manifest_sha256 = "<SHA-256 of the manifest>"

   [user]
   registration_terms_version = "2026-10"
   registration_terms_from = "2026-12-09T00:00:00+01:00"
   ```

   `terms_version` in `[user]` stays `2026-09-r4`. Run `academy check-config` with the new
   configuration before the restart, as the service user `academy`: the `academy` command the
   NixOS module installs switches to that user by itself (via `sudo -u academy`); any other
   binary needs `sudo -u academy` in front. The service reads the directory as that user under
   `ProtectSystem=strict`, so a file it may not read fails here and not at the restart.
   `check-config` loads the manifest and both PDFs, checks every SHA-256 and compares the two
   starts, as `serve` does. Build the frontend with the same
   `NUXT_PUBLIC_TERMS_2026_10_FROM`. Deploy manually as usual. Until the start, users see only
   the two announced pages.
5. **At the start** nothing has to be done.
6. **Check both sides.** Before the start, compare the frontend's start with the backend's:
   `NUXT_PUBLIC_TERMS_2026_10_FROM` of the deployed build (in the generated `index.html`,
   `TERMS_2026_10_FROM`) has to name the same moment as `effective_from` in the manifest and
   `registration_terms_from`. The backend compares its two values itself; the frontend value is
   a separate build input, so this comparison is a step of its own. After the start check:
   `/docs/terms-and-conditions` shows "Fassung: 2026-10", `/docs/right-of-withdrawal` shows
   "Stand: Fassung 2026-10" and the English translation, a new signup records `2026-10`, a new
   offer names `"terms_version": "2026-10"`, and the PDFs attached to a new order have the
   SHA-256 values of the manifest.

Existing accounts are never asked to accept 2026-10. The frontend has not mounted the gate for
older accounts since 11 September 2026 (frontend commit `3a01f90e`, `docs/CONSUMER_UI.md`), so
nobody is asked for 2026-09-r4 either; its retained code also ends at the start. An account
that had not accepted 2026-09-r4 keeps the terms it agreed to (AGB 20.3), and every order
binds the version its offer names. The Daily cohort keeps its own
document bundle; switching Daily on after the start needs its own decision.
