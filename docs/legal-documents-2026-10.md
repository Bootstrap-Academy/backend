# Terms 2026-10: start, PDFs and bundle

AGB and Widerrufsbelehrung 2026-10 take effect at one moment, the start. Everything below uses
that one value: an ISO timestamp with an offset that is 00:00 in Europe/Berlin, for example
`2026-12-09T00:00:00+01:00` in winter or `2027-06-01T00:00:00+02:00` in summer. Frontend and
this script refuse any other moment, so the date in the texts and the moment of the switch are
the same day.

| Where | What the start does |
|---|---|
| Frontend build, `NUXT_PUBLIC_TERMS_2026_10_FROM` | Unset: the 2026-10 pages do not exist. Set: `/docs/terms-and-conditions-2026-10` and `/docs/right-of-withdrawal-2026-10` exist and say until the start that they do not apply yet. From the start the main pages show 2026-10 and signup offers 2026-10. The browser decides by its clock, so the switch needs no new build. |
| `scripts/render-purchase-documents.py --release 2026-10` | Renders both PDFs with the date in the text, checks the whole text with `pdftotext` and writes a manifest with both SHA-256 values. |
| Backend configuration | The order documents bundle (`LearningDocumentBundleConfig`: version, absolute paths, SHA-256) and the registration terms with the same start. |

## Steps

1. **Test acceptance.** Choose a provisional start for Test only. Build the frontend of Test
   with `NUXT_PUBLIC_TERMS_2026_10_FROM` set to it, render the PDFs (step 3) from exactly the
   deployed frontend commit, and configure the backend of Test with the same start and bundle.
   A start in the past shows the state after the switch, one in the future the announcement.
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
   manifest records the frontend commit; a `-dirty` suffix means the checkout had changes and
   the PDFs must not be used.
4. **Prod configuration, before the start.** Copy `agb-2026-10.pdf` and
   `widerrufsbelehrung-2026-10.pdf` to the host. Set the bundle to their absolute paths and to
   `bundle.terms_sha256` and `bundle.withdrawal_sha256` of the manifest; the backend refuses a
   file whose SHA-256 differs. Set the registration terms to `2026-10` with the same start. Build
   the Prod frontend with the same `NUXT_PUBLIC_TERMS_2026_10_FROM`. Deploy manually as usual.
   Until the start, users see only the two announced pages.
5. **At the start** nothing has to be done.
6. **After the start** check on Prod: `/docs/terms-and-conditions` shows "Fassung: 2026-10",
   `/docs/right-of-withdrawal` shows "Stand: Fassung 2026-10" and the English translation, a new
   signup records `2026-10`, and the PDFs attached to a new order have the SHA-256 values of the
   manifest.

Existing accounts are never asked to accept 2026-10: the gate for older accounts keeps asking
for 2026-09-r4 and links its page `/docs/terms-and-conditions-2026-09-r4`.
