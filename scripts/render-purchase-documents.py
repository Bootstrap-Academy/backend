"""Render the exact sibling frontend legal templates using only local assets.

Run from the multi-repository workspace. Existing accepted purchase/renewal PDF
snapshots are stored independently; this changes prospective embedded assets only.
"""

import argparse
import hashlib
import html
import json
import os
import re
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CSS = """@page { size: A4; margin: 18mm 18mm 18mm 20mm; }
body { color: #181818; font: 11pt/1.5 Arial, sans-serif; }
h1 { font-size: 20pt; } h2 { font-size: 14pt; margin-top: 1.3em; }
h3 { font-size: 12pt; } h1,h2,h3 { break-after: avoid; }
p,li { orphans: 3; widows: 3; } a { color: inherit; text-decoration: underline; }
"""
PDFTEXT = os.environ.get("PDFTOTEXT", "pdftotext")


def plain(value):
    return re.sub(r"\s+", " ", html.unescape(re.sub("<[^>]*>", " ", value))).strip()


def normal(value):
    return re.sub(r"\s+", "", plain(value)).replace("ﬁ", "fi").replace("ﬂ", "fl")


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--release", required=True)
parser.add_argument("--replace-unpublished", action="store_true")
parser.add_argument("--reuse-withdrawal-release", help="Reuse the unchanged withdrawal document from an existing verified manifest")
args = parser.parse_args()
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
        previous = json.loads((ROOT / f"backend/academy_assets/assets/email/purchase-document-manifest-{previous_release}.json").read_text())
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
        lines = [f"Unveröffentlichte vorgesehene gemeinsame Fassung {release}. Kein Wirksamkeitstermin oder Zustimmungsnachweis.", ""]
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
manifest_path.write_text(
    json.dumps(manifest, ensure_ascii=False, indent=2) + "\n"
)
