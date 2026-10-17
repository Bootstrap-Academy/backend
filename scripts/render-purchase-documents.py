"""Render the exact sibling frontend legal templates using only local assets.

Run from the multi-repository workspace. Existing accepted purchase/renewal PDF
snapshots are stored independently; this changes prospective embedded assets only.

Release 2026-10 is bound through a configured document bundle with absolute paths
and pinned SHA-256 values, so its PDFs are written to an output directory outside
this repository together with a manifest. Steps: docs/legal-documents-2026-10.md.
"""

import argparse
import hashlib
import html
import json
import os
import re
import subprocess
import sys
import tempfile
from datetime import datetime
from pathlib import Path
from zoneinfo import ZoneInfo

ROOT = Path(__file__).resolve().parents[2]
BACKEND = Path(__file__).resolve().parents[1]
CSS = """@page { size: A4; margin: 18mm 18mm 18mm 20mm; }
body { color: #181818; font: 11pt/1.5 Arial, sans-serif; }
h1 { font-size: 20pt; } h2 { font-size: 14pt; margin-top: 1.3em; }
h3 { font-size: 12pt; } h1,h2,h3 { break-after: avoid; }
p,li { orphans: 3; widows: 3; } a { color: inherit; text-decoration: underline; }
"""
# The English translation of the withdrawal instruction 2026-10 sits two heading levels down.
CSS_2026_10 = (
    CSS + "h4 { font-size: 11pt; } h5 { font-size: 11pt; font-style: italic; } h4,h5 { break-after: avoid; }\n"
)
PDFTEXT = os.environ.get("PDFTOTEXT", "pdftotext")


def plain(value):
    return re.sub(r"\s+", " ", html.unescape(re.sub("<[^>]*>", " ", value))).strip()


def normal(value):
    return re.sub(r"\s+", "", plain(value)).replace("ﬁ", "fi").replace("ﬂ", "fl")


def chromium_pdf(document, output, profile):
    subprocess.run(
        [
            os.environ.get("CHROMIUM", "chromium"),
            "--headless",
            "--no-sandbox",
            "--disable-gpu",
            "--disable-background-networking",
            "--disable-component-update",
            "--host-resolver-rules=MAP * ~NOTFOUND",
            "--no-pdf-header-footer",
            f"--user-data-dir={profile}",
            f"--print-to-pdf={output}",
            document.as_uri(),
        ],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        timeout=50,
    )


# The same rules as frontend utils/legalEffectiveDate.ts: an ISO timestamp with an offset that is
# 00:00 in Europe/Berlin, written from the calendar day in Berlin with fixed month names.
ISO_TIMESTAMP = re.compile(
    r"\d{4}-\d{2}-\d{2}T(?:[01]\d|2[0-3]):[0-5]\d(?::[0-5]\d(?:\.\d{1,3})?)?(?:Z|[+-](?:[01]\d|2[0-3]):[0-5]\d)"
)
MONTHS_DE = "Januar Februar März April Mai Juni Juli August September Oktober November Dezember".split()
MONTHS_EN = "January February March April May June July August September October November December".split()


def effective_date(value):
    """The German and the English date of a start of terms, as the frontend pages write them."""
    try:
        assert ISO_TIMESTAMP.fullmatch(value)
        start = datetime.fromisoformat(value)
    except (AssertionError, ValueError):
        raise SystemExit(
            f'Effective date "{value}" is not an ISO timestamp with an offset, such as 2026-12-09T00:00:00+01:00'
        )
    day = start.astimezone(ZoneInfo("Europe/Berlin"))
    if (day.hour, day.minute, day.second, day.microsecond) != (0, 0, 0, 0):
        raise SystemExit(f'Effective date "{value}" is not 00:00 in Europe/Berlin')
    return f"{day.day}. {MONTHS_DE[day.month - 1]} {day.year}", f"{day.day} {MONTHS_EN[day.month - 1]} {day.year}"


def render_2026_10(args):
    """Render AGB and Widerrufsbelehrung 2026-10 with their start into a directory outside the repository."""
    if not args.effective_date or not args.output_dir:
        raise SystemExit("Release 2026-10 needs --effective-date and --output-dir")
    german, english = effective_date(args.effective_date)
    frontend = args.frontend.resolve()
    output_dir = args.output_dir.resolve()
    if output_dir.is_relative_to(BACKEND):
        raise SystemExit("The 2026-10 PDFs are bound through the configured bundle; write them outside the repository")
    if output_dir.exists() and any(output_dir.iterdir()):
        raise SystemExit(f"Refusing to write into the non-empty directory {output_dir}")
    output_dir.mkdir(parents=True, exist_ok=True)
    revision = subprocess.run(
        ["git", "-C", str(frontend), "rev-parse", "HEAD"], check=True, capture_output=True, text=True
    ).stdout.strip()
    dirty = subprocess.run(["git", "-C", str(frontend), "status", "--porcelain"], capture_output=True, text=True).stdout
    manifest = {
        "release": "2026-10",
        "rendering": "local static HTML, A4",
        "effective_from": args.effective_date,
        "effective_date_de": german,
        "effective_date_en": english,
        "frontend_commit": revision + ("-dirty" if dirty.strip() else ""),
        "documents": [],
    }
    for component, stem, title in [
        ("components/legal/TermsAndConditions202610.vue", "agb", "Allgemeine Geschäftsbedingungen"),
        ("components/legal/RightOfWithdrawal202610.vue", "widerrufsbelehrung", "Widerrufsbelehrung"),
    ]:
        source = frontend / component
        template = re.search(r"<template>(.*?)</template>", source.read_text(), re.S).group(1)
        template = template.replace("{{ effectiveDate }}", german).replace("{{ effectiveDateEn }}", english)
        assert not any(s in template for s in ("{{", "<Nuxt", "<img", "<script", "<iframe", "<link", "<style"))
        assert german in template, "the German date is part of the text"
        if stem == "widerrufsbelehrung":
            translation = template.split('<article id="english" lang="en">', 1)
            assert (
                len(translation) == 2 and english in translation[1]
            ), "the English translation follows the German text"
            assert len(re.findall(r"<p(?:\s[^>]*)?>", translation[1])) > 20, "the English translation is not empty"
        output = output_dir / f"{stem}-2026-10.pdf"
        with tempfile.TemporaryDirectory(prefix="bootstrap-2026-10-pdf-") as tmp:
            base = Path(tmp)
            document = base / "document.html"
            document.write_text(
                f'<!doctype html><html lang="de"><meta charset="utf-8"><title>{title} – Fassung 2026-10</title><style>{CSS_2026_10}</style>{template}</html>'
            )
            chromium_pdf(document, output, base / "profile")
            textfile = base / "pdf.txt"
            subprocess.run([PDFTEXT, "-layout", str(output), str(textfile)], check=True)
            actual = normal(textfile.read_text())
        blocks = re.findall(r"<(h[1-6]|p|li)(?:\s[^>]*)?>(.*?)</\1>", template, re.S)
        assert all(normal(value) in actual for _, value in blocks), "PDF omitted source text"
        assert actual == normal(template), "whole PDF differs from whole template"
        pdf_sha256 = hashlib.sha256(output.read_bytes()).hexdigest()
        manifest["documents"].append(
            {
                "document_release": "2026-10",
                "source": f"frontend/{component}",
                "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                "pdf": output.name,
                "pdf_sha256": pdf_sha256,
                "verified_blocks": len(blocks),
                "title": title + " – Fassung 2026-10",
            }
        )
        print(f"PASS {stem}: {len(blocks)} exact source blocks and the whole text in {output}")
        print(f"     SHA-256 {pdf_sha256}")
    terms, withdrawal = manifest["documents"]
    # The values of the configured bundle (`LearningDocumentBundleConfig`); the paths become the
    # absolute paths of these files where they are deployed.
    manifest["bundle"] = {
        "terms_version": "2026-10",
        "terms_pdf": terms["pdf"],
        "terms_sha256": terms["pdf_sha256"],
        "withdrawal_pdf": withdrawal["pdf"],
        "withdrawal_sha256": withdrawal["pdf_sha256"],
    }
    (output_dir / "purchase-document-manifest-2026-10.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n"
    )
    print(f"PASS manifest {output_dir / 'purchase-document-manifest-2026-10.json'}")


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--release", required=True)
parser.add_argument("--replace-unpublished", action="store_true")
parser.add_argument(
    "--reuse-withdrawal-release", help="Reuse the unchanged withdrawal document from an existing verified manifest"
)
parser.add_argument(
    "--effective-date",
    help="Release 2026-10: its start, the value of NUXT_PUBLIC_TERMS_2026_10_FROM (00:00 Europe/Berlin)",
)
parser.add_argument(
    "--frontend", type=Path, default=ROOT / "frontend", help="Release 2026-10: the frontend checkout to render from"
)
parser.add_argument("--output-dir", type=Path, help="Release 2026-10: new or empty directory outside the repository")
args = parser.parse_args()
if args.release == "2026-10":
    render_2026_10(args)
    sys.exit(0)
assert not (args.effective_date or args.output_dir), "--effective-date and --output-dir belong to release 2026-10"
assert re.fullmatch(r"2026-09-r[1-9][0-9]*", args.release), "Original accepted release is immutable"
release = args.release
manifest = {"release": release, "rendering": "local static HTML, A4", "documents": []}
manifest_path = ROOT / f"backend/academy_assets/assets/email/purchase-document-manifest-{release}.json"
assert not manifest_path.exists() or args.replace_unpublished, "Refusing to overwrite an existing manifest"
for page, stem, title in [
    ("terms-and-conditions", "agb", "Allgemeine Geschäftsbedingungen"),
    ("right-of-withdrawal", "widerrufsbelehrung", "Widerrufsbelehrung"),
]:
    if stem == "widerrufsbelehrung" and args.reuse_withdrawal_release:
        previous_release = args.reuse_withdrawal_release
        assert re.fullmatch(r"2026-09-r[1-9][0-9]*", previous_release)
        previous = json.loads(
            (
                ROOT / f"backend/academy_assets/assets/email/purchase-document-manifest-{previous_release}.json"
            ).read_text()
        )
        assert previous["release"] == previous_release
        matches = [doc for doc in previous["documents"] if doc["source"] == f"frontend/pages/docs/{page}.vue"]
        assert len(matches) == 1
        document = matches[0]
        assert hashlib.sha256((ROOT / document["source"]).read_bytes()).hexdigest() == document["source_sha256"]
        assert hashlib.sha256((ROOT / document["pdf"]).read_bytes()).hexdigest() == document["pdf_sha256"]
        manifest["documents"].append({**document, "document_release": previous_release, "reused_unchanged": True})
        print(f"PASS {stem}: unchanged {previous_release} source/PDF hashes; no rendering or overwrite")
        continue
    draft = f"{stem}-{release}.md"
    source = ROOT / f"frontend/pages/docs/{page}.vue"
    template = re.search(r"<template>(.*?)</template>", source.read_text(), re.S).group(1)
    assert not any(s in template for s in ("{{", "<Nuxt", "<img", "<script", "<iframe", "<link", "<style"))
    output = ROOT / f"backend/academy_assets/assets/email/{stem}-{release}.pdf"
    assert not output.exists() or args.replace_unpublished, "Refusing to overwrite an existing release artifact"
    with tempfile.TemporaryDirectory(prefix="bootstrap-l2-pdf-") as tmp:
        base = Path(tmp)
        document = base / "document.html"
        document.write_text(
            f'<!doctype html><html lang="de"><meta charset="utf-8"><title>{title} – Fassung {release}</title><style>{CSS}</style>{template}</html>'
        )
        subprocess.run(
            [
                os.environ.get("CHROMIUM", "chromium"),
                "--headless",
                "--no-sandbox",
                "--disable-gpu",
                "--disable-background-networking",
                "--disable-component-update",
                "--host-resolver-rules=MAP * ~NOTFOUND",
                "--no-pdf-header-footer",
                f"--user-data-dir={base / 'profile'}",
                f"--print-to-pdf={output}",
                document.as_uri(),
            ],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=50,
        )
        textfile = base / "pdf.txt"
        subprocess.run([PDFTEXT, "-layout", str(output), str(textfile)], check=True)
        actual = normal(textfile.read_text())
        blocks = re.findall(r"<(h[1-3]|p|li)(?:\s[^>]*)?>(.*?)</\1>", template, re.S)
        assert all(normal(value) in actual for _, value in blocks), "PDF omitted source text"
        lines = [
            f"Unveröffentlichte vorgesehene gemeinsame Fassung {release}. Kein Wirksamkeitstermin oder Zustimmungsnachweis.",
            "",
        ]
        for tag, value in blocks:
            prefix = "#" * int(tag[1]) + " " if tag.startswith("h") else "- " if tag == "li" else ""
            lines.extend([prefix + plain(value), ""])
        (ROOT / "legal/2026-09-complaint/drafts" / draft).write_text("\n".join(lines))
        manifest["documents"].append(
            {
                "document_release": release,
                "source": str(source.relative_to(ROOT)),
                "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                "pdf": str(output.relative_to(ROOT)),
                "pdf_sha256": hashlib.sha256(output.read_bytes()).hexdigest(),
                "verified_blocks": len(blocks),
                "title": title + " – Fassung " + release,
            }
        )
        print(f"PASS {stem}: {len(blocks)} exact source blocks in PDF; prospective draft refreshed")
manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
