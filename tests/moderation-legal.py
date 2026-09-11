"""Local immutable-original and prospective HTML/draft/PDF parity checks."""

import hashlib, html, json, re, subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PDFTEXT = "/nix/store/qkfcppix6b8jzga6y76yblpvkhvpnhzz-poppler-utils-26.06.0/bin/pdftotext"


def sha(value):
    return hashlib.sha256(value).hexdigest()


def plain(value):
    return re.sub(r"\s+", " ", html.unescape(re.sub("<[^>]*>", " ", value))).strip()


def normal(value):
    return re.sub(r"\s+", "", plain(value)).replace("ﬁ", "fi").replace("ﬂ", "fl")


for release in ["2026-09-r1", "2026-09-r2"]:
    manifest = json.loads(
        (ROOT / f"backend/academy_assets/assets/email/purchase-document-manifest-{release}.json").read_text()
    )
    assert manifest["release"] == release
    assert len(manifest["documents"]) == 2
    for doc in manifest["documents"]:
        document_release = doc.get("document_release", release)
        source = ROOT / doc["source"]
        if release == "2026-09-r1" and source.name == "terms-and-conditions.vue":
            source = ROOT / "frontend/components/legal/TermsAndConditionsR1.vue"
        pdf = ROOT / doc["pdf"]
        assert sha(source.read_bytes()) == doc["source_sha256"]
        assert sha(pdf.read_bytes()) == doc["pdf_sha256"]
        template = re.search(r"<template>(.*?)</template>", source.read_text(), re.S)[1]
        actual = subprocess.check_output([PDFTEXT, "-layout", str(pdf), "-"], text=True)
        assert normal(actual) == normal(template), (pdf, "whole PDF differs from whole template")
        blocks = re.findall(r"<(h[1-3]|p|li)(?:\s[^>]*)?>(.*?)</\1>", template, re.S)
        assert len(blocks) == doc["verified_blocks"]
        draft = ROOT / "legal/2026-09-complaint/drafts" / pdf.with_suffix(".md").name
        lines = [
            f"Unveröffentlichte vorgesehene gemeinsame Fassung {document_release}. Kein Wirksamkeitstermin oder Zustimmungsnachweis.",
            "",
        ]
        for tag, value in blocks:
            lines.extend(
                [("#" * int(tag[1]) + " " if tag.startswith("h") else "- " if tag == "li" else "") + plain(value), ""]
            )
        assert draft.read_text() == "\n".join(lines)
        print("PASS whole HTML/PDF and exact draft/hash:", release, pdf.name, len(blocks), "blocks")
    if release == "2026-09-r2":
        agb, withdrawal = manifest["documents"]
        assert agb["document_release"] == "2026-09-r2"
        assert withdrawal["document_release"] == "2026-09-r1" and withdrawal["reused_unchanged"] is True
        previous = json.loads(
            (ROOT / "backend/academy_assets/assets/email/purchase-document-manifest-2026-09-r1.json").read_text()
        )["documents"][1]
        assert all(withdrawal[k] == v for k, v in previous.items())
old_manifest = "academy_assets/assets/email/purchase-document-manifest.json"
for relative in [
    old_manifest,
    "academy_assets/assets/email/agb-2026-09.pdf",
    "academy_assets/assets/email/widerrufsbelehrung-2026-09.pdf",
]:
    old = subprocess.check_output(
        ["git", "show", "11386a6d15f94a601b18ea5205998725e228b895:" + relative], cwd=ROOT / "backend"
    )
    assert old == (ROOT / "backend" / relative).read_bytes()
    print("PASS committed final-L1 original bytes unchanged:", relative, sha(old))
source = ROOT / "frontend/pages/docs/privacy.vue"
template = re.search(r"<template>(.*?)</template>", source.read_text(), re.S)[1]
blocks = []
headers = 0
for tag, body in re.findall(r"<(h[1-3]|p|li|td|th)(?:\s[^>]*)?>(.*?)</\1>", template, re.S):
    value = " ".join(html.unescape(re.sub("<[^>]+>", "", body)).split())
    if not value:
        continue
    if tag == "th":
        headers += 1
    prefix = (
        "#" * int(tag[1]) + " "
        if tag[0] == "h" and tag != "th"
        else "- " if tag == "li" else "**Tabellenspalte:** " if tag == "th" else ""
    )
    blocks.append(prefix + value)
expected = (
    "Unveröffentlichte vorgesehene gemeinsame Fassung 2026-09-r2. Kein Wirksamkeitstermin. Quelle: frontend/pages/docs/privacy.vue; SHA256 "
    + sha(source.read_bytes())
    + "\n\n"
    + "\n\n".join(blocks)
    + "\n"
)
assert (ROOT / "legal/2026-09-complaint/drafts/privacy-notice-2026-09-r2.md").read_text() == expected
assert headers == 21 and len(blocks) == 514
for folder in ["frontend/pages", "frontend/components", "frontend/locales", "backend/academy_templates"]:
    for p in (ROOT / folder).rglob("*"):
        if p.is_file() and p.suffix in [".vue", ".ts", ".json", ".html", ".txt", ".j2"]:
            assert "secjur" not in p.read_text().lower(), p
assert not re.search("keinem Konto und keiner Person mehr zugeordnet werden kann", template)
assert (
    "mindestens sechs Kalendermonate"
    in (ROOT / "backend/academy_persistence/postgres/migrations/2026-09-08-120000_moderation/up.sql").read_text()
)
print(
    "PASS exact privacy draft: all514 ordered blocks including21 table headers; current source hash, consistent pseudonym wording, actual acknowledgment minimum-calendar wording, publishable templates SECJUR-free"
)
