#!/usr/bin/env python3
"""Generate the conservative GS1 host profile using Python's Unicode database.

This is not an IDNA/UTS #46 implementation. Input data is the Unicode 15.0.0
character category, compatibility normalization, case and bidi data exposed by
Python's stdlib unicodedata. Unicode data: https://www.unicode.org/license.txt
Runtime code does not call Python, normalize Unicode, or load external tables.
Run using a Python whose unicodedata.unidata_version is exactly 15.0.0.
"""
from pathlib import Path
import hashlib
import json
import sys
import unicodedata as ud

VERSION = "15.0.0"
ROOT = Path(__file__).resolve().parents[1]
DESTINATION = ROOT / "src/idna_table.rs"
if ud.unidata_version != VERSION:
    raise SystemExit(f"Expected Unicode {VERSION}; found {ud.unidata_version}")


def ranges(values):
    result = []
    for value in values:
        if result and value == result[-1][1] + 1:
            result[-1][1] = value
        else:
            result.append([value, value])
    return result


rejected = []
lowercase = []
rtl_letters = []
arabic_digits = []
for code in range(0x110000):
    scalar = chr(code)
    category = ud.category(scalar)
    if code >= 128 and (
        category[0] in "MCZ"
        or ud.normalize("NFKC", scalar) != scalar
        or (scalar.casefold() != scalar.lower() and scalar not in "ßς")
        or 0x2FF0 <= code <= 0x2FFF
        or code in (0xFFFC, 0xFFFD)
        or 0x1100 <= code <= 0x11FF
        or 0xA960 <= code <= 0xA97F
        or 0xD7B0 <= code <= 0xD7FF
    ):
        rejected.append(code)
    lower = scalar.lower()
    if lower != scalar:
        lowercase.append([code, [ord(c) for c in lower]])
    if ud.bidirectional(scalar) in ("R", "AL"):
        rtl_letters.append(code)
    if ud.bidirectional(scalar) == "AN":
        arabic_digits.append(code)

records = {
    "unicode": VERSION,
    "rejected": ranges(rejected),
    "lowercase": lowercase,
    "rtlLetters": ranges(rtl_letters),
    "arabicDigits": ranges(arabic_digits),
}
checksum = hashlib.sha256(json.dumps(records, ensure_ascii=True, separators=(",", ":")).encode()).hexdigest()
output = [
    "//! Generated conservative hostname data; not complete IDNA or UTS #46.",
    f"//! Unicode data version: {VERSION}; derived via Python stdlib unicodedata.",
    "//! Generator: tools/generate_idna_profile.py. Unicode data license:",
    "//! <https://www.unicode.org/license.txt>; redistributed as LICENSE-UNICODE.",
    f"//! Semantic-record SHA-256: {checksum}",
    "// Reject marks, other/control/format/unassigned, separators, NFKC-unstable",
    "// scalars, non-deviation casefold changes, ideographic description operators,",
    "// object/replacement symbols, and Hangul Jamo (contextual NFC composition).",
]
for name, key in [("REJECTED", "rejected"), ("RTL_LETTERS", "rtlLetters"), ("ARABIC_DIGITS", "arabicDigits")]:
    output.append(f"pub(super) static {name}: &[(u32, u32)] = &[")
    for start, end in records[key]:
        output.append(f"    (0x{start:X}, 0x{end:X}),")
    output.append("];\n")
output.append("pub(super) static LOWERCASE: &[(u32, &str)] = &[")
for code, points in lowercase:
    encoded = "".join(f"\\u{{{point:X}}}" for point in points)
    output.append(f'    (0x{code:X}, "{encoded}"),')
output.append("];\n")
content = "\n".join(output)
if len(sys.argv) > 1 and sys.argv[1] == "--check":
    if DESTINATION.read_text(encoding="utf-8") != content:
        raise SystemExit("idna_table.rs differs; regenerate using the pinned Unicode database")
    print("Unicode profile tables match the pinned generator")
else:
    DESTINATION.write_text(content, encoding="utf-8", newline="\n")
    print(f"Wrote {DESTINATION.name}: {len(records['rejected'])} rejection ranges, {len(lowercase)} mappings; {checksum}")
