#!/usr/bin/env python3
"""Machine-translate the Strong's definitions into German, via the Batch API.

    ANTHROPIC_API_KEY=sk-... python3 data-prep/strongs-lang/translate.py

Translates the two English prose fields of every data/strongs.json entry —
`strongs_def` and `derivation` — and writes the result to
`data-prep/strongs-lang/translations.de.json` (an intermediate this directory
commits; `build-strongs.py` folds it into the shipped pack file).
`kjv_def` is NOT translated: its German counterpart is the Luther renderings,
derived from the tagged corpus by `build-strongs.py` — real data, not
translation.

Idempotent and resumable: already-translated codes are skipped, so an
interrupted run (or a later strongs.json addition) only pays for what is
missing. One full run is ~355 batch requests over ~14k entries and costs a few
dollars at batch rates; the batch usually completes within the hour.

The source (Strong's, 1890) is public domain; this translation is therefore
ours to ship freely — labelled as machine-translated in the app, with a
pointer to the repo's issues for corrections (the maintainer's call, 2026-08-11).

No third-party dependencies — stdlib urllib only — so it runs anywhere the
other data-prep scripts do.
"""

import json
import os
import sys
import time
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
STRONGS = ROOT / "data" / "strongs.json"
OUT = Path(__file__).resolve().parent / "translations.json"

API = "https://api.anthropic.com/v1"
MODEL = "claude-opus-5"
CHUNK = 40

SYSTEM = """You translate entries of Strong's Exhaustive Concordance dictionary (1890) from English to German for a Bible-study app. The register is a concise, traditional German lexicon.

Rules:
- Translate the meaning faithfully and tersely; keep the original punctuation structure (semicolons separating senses, parentheses for glosses).
- Keep Hebrew, Greek, and Latin words, transliterations, and proper names exactly as written.
- Keep cross-reference markers like "compare H7225" as "vergleiche H7225"; "see G26" as "siehe G26".
- Use these fixed renderings for Strong's formulaic vocabulary:
  properly → eigentlich; figuratively → übertragen; by implication → folgernd; by extension → erweitert; causatively → kausativ; specifically → speziell; generically → allgemein; literally → wörtlich; morally → moralisch; a primitive root → eine Wurzel (Primitivum); a primitive word → ein Primitivwort; i.e. → d. h.; especially → besonders; abstractly → abstrakt; collectively → kollektiv; intensively → intensiv; euphemistically → euphemistisch; adverbially → adverbial; used of → gebraucht für.
- German noun capitalization applies; otherwise keep the lexicon's lowercase style.
- Return ONLY the fields you are given per entry; never invent content for missing fields."""

SCHEMA = {
    "type": "object",
    "properties": {
        "entries": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "code": {"type": "string"},
                    "derivation": {"type": "string"},
                    "strongs_def": {"type": "string"},
                },
                "required": ["code"],
                "additionalProperties": False,
            },
        }
    },
    "required": ["entries"],
    "additionalProperties": False,
}


def call(path: str, payload=None, method="POST"):
    req = urllib.request.Request(
        API + path,
        data=json.dumps(payload).encode() if payload is not None else None,
        method=method if payload is not None or method != "POST" else "POST",
        headers={
            "x-api-key": os.environ["ANTHROPIC_API_KEY"],
            "anthropic-version": "2023-06-01",
            "content-type": "application/json",
        },
    )
    with urllib.request.urlopen(req) as r:
        return json.loads(r.read())


def fetch_url(url: str) -> str:
    req = urllib.request.Request(
        url,
        headers={"x-api-key": os.environ["ANTHROPIC_API_KEY"], "anthropic-version": "2023-06-01"},
    )
    with urllib.request.urlopen(req) as r:
        return r.read().decode()


def main() -> int:
    if "ANTHROPIC_API_KEY" not in os.environ:
        print("set ANTHROPIC_API_KEY first", file=sys.stderr)
        return 2

    strongs = json.loads(STRONGS.read_text(encoding="utf-8"))
    done = json.loads(OUT.read_text(encoding="utf-8")) if OUT.exists() else {}

    todo = []
    for code, e in sorted(strongs.items()):
        if code in done:
            continue
        fields = {k: e[k] for k in ("derivation", "strongs_def") if e.get(k)}
        if not fields:
            done[code] = {}  # nothing to translate; record so we never re-ask
            continue
        todo.append((code, fields))
    if not todo:
        OUT.write_text(json.dumps(done, ensure_ascii=False, indent=0, sort_keys=True), encoding="utf-8")
        print(f"nothing to translate — {len(done)} codes already covered")
        return 0
    print(f"{len(todo)} entries to translate ({len(done)} already done)")

    requests = []
    for i in range(0, len(todo), CHUNK):
        chunk = todo[i : i + CHUNK]
        prompt = "Translate these Strong's entries to German. Return every entry, same codes.\n\n" + json.dumps(
            [{"code": c, **f} for c, f in chunk], ensure_ascii=False, indent=1
        )
        requests.append(
            {
                "custom_id": f"chunk-{i}",
                "params": {
                    "model": MODEL,
                    "max_tokens": 8000,
                    "system": [{"type": "text", "text": SYSTEM, "cache_control": {"type": "ephemeral"}}],
                    "messages": [{"role": "user", "content": prompt}],
                    "output_config": {"format": {"type": "json_schema", "schema": SCHEMA}},
                },
            }
        )

    batch = call("/messages/batches", {"requests": requests})
    print(f"batch {batch['id']} submitted ({len(requests)} requests); polling...")
    while True:
        time.sleep(60)
        batch = call(f"/messages/batches/{batch['id']}", method="GET")
        c = batch["request_counts"]
        print(f"  {batch['processing_status']}: {c['succeeded']} ok, {c['errored']} err, {c['processing']} left")
        if batch["processing_status"] == "ended":
            break

    errored = 0
    for line in fetch_url(batch["results_url"]).splitlines():
        result = json.loads(line)
        if result["result"]["type"] != "succeeded":
            errored += 1
            continue
        text = next(b["text"] for b in result["result"]["message"]["content"] if b["type"] == "text")
        for entry in json.loads(text)["entries"]:
            code = entry.pop("code")
            if code in strongs:
                done[code] = {k: v for k, v in entry.items() if v}

    OUT.write_text(json.dumps(done, ensure_ascii=False, indent=0, sort_keys=True), encoding="utf-8")
    missing = [c for c, _ in todo if c not in done]
    print(f"wrote {OUT.relative_to(ROOT)}: {len(done)} codes covered, {errored} request(s) errored")
    if missing:
        print(f"{len(missing)} codes still missing (e.g. {missing[:5]}) — re-run to retry them")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
