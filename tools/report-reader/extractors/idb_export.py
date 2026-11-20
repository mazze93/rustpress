#!/usr/bin/env python3
"""Offline Chromium IndexedDB reader: LevelDB records + blob store.

Read-only against inputs. Run it on a snapshot copied from a CLOSED app/browser
profile. Idempotent: the same inputs always produce byte-identical output, and
a re-run replaces a previous export (and only a previous export).

Outputs in --out (format version 2):
  export.json     pretty, sorted-key metadata for every entry and blob; no raw
                  bytes, so its size tracks entry count, not value size
  records.jsonl   full fidelity: one {"id", "key_b64", "value_b64"} per line,
                  joined to export.json by "id" (value_b64 null for tombstones)

History is kept by default. Each entry has a status: live, superseded (older
value, key still live), deleted (older value of a deleted key), or tombstone
(the deletion marker). Only history LevelDB has not compacted away survives,
so missing history proves nothing. --live-only restores live-state output.
  dashboard.json  compact, size-bounded view for the Forensic Dashboard
                  (tools/report-reader): totals + at most --preview-rows rows
  report.txt      plain-text tables (also printed to stdout)
  blobs/<sha256>  content-addressed copies of blobs <= --max-blob-mib
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import shutil
import struct
import sys
from collections import Counter
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from pathlib import Path
from typing import Any

TOOL = "idb-export"
FORMAT_VERSION = 2
MARKER = ".idb-export"  # marks a directory this tool owns and may replace

TYPE_DELETION, TYPE_VALUE = 0, 1
LOG_BLOCK = 32768
LOG_HEADER = 7
TABLE_FOOTER = 48
BLOCK_TRAILER = 5
TABLE_MAGIC = struct.pack("<Q", 0xDB4775248B80FB57)
CHUNK = 1 << 20
SNIFF_BYTES = 64

DEFAULT_PREVIEW_ROWS = 2000
PREVIEW_STRINGS = 4  # value_strings kept per dashboard row
PREVIEW_CHARS = 64  # characters kept per preview string
KEY_CHARS = 40  # characters kept per displayed key
MAX_WARNINGS = 50  # warnings carried into dashboard.json

Json = dict[str, Any]


# ---------------------------------------------------------------- primitives


def read_varint(buf: bytes, pos: int) -> tuple[int, int]:
    result = shift = 0
    while True:
        if pos >= len(buf):
            raise ValueError("truncated varint")
        b = buf[pos]
        pos += 1
        result |= (b & 0x7F) << shift
        if not b & 0x80:
            return result, pos
        shift += 7
        if shift > 63:
            raise ValueError("varint too long")


def read_slice(buf: bytes, pos: int) -> tuple[bytes, int]:
    n, pos = read_varint(buf, pos)
    end = pos + n
    if end > len(buf):
        raise ValueError("truncated slice")
    return buf[pos:end], end


def snappy_decompress(src: bytes) -> bytes:
    """Raw (unframed) Snappy, as used by LevelDB table blocks."""
    expected, pos = read_varint(src, 0)
    out = bytearray()
    n = len(src)
    while pos < n:
        tag = src[pos]
        pos += 1
        kind = tag & 3
        if kind == 0:  # literal
            ln = tag >> 2
            if ln >= 60:
                nb = ln - 59
                ln = int.from_bytes(src[pos : pos + nb], "little")
                pos += nb
            ln += 1
            if pos + ln > n:
                raise ValueError("snappy literal overrun")
            out += src[pos : pos + ln]
            pos += ln
            continue
        if kind == 1:
            ln = 4 + ((tag >> 2) & 7)
            off = ((tag >> 5) << 8) | src[pos]
            pos += 1
        elif kind == 2:
            ln = (tag >> 2) + 1
            off = int.from_bytes(src[pos : pos + 2], "little")
            pos += 2
        else:
            ln = (tag >> 2) + 1
            off = int.from_bytes(src[pos : pos + 4], "little")
            pos += 4
        if off == 0 or off > len(out):
            raise ValueError("snappy bad copy offset")
        start = len(out) - off
        if ln <= off:
            out += out[start : start + ln]
        else:  # overlapping copy repeats the tail
            for i in range(ln):
                out.append(out[start + i])
    if len(out) != expected:
        raise ValueError("snappy length mismatch")
    return bytes(out)


# ---------------------------------------------------------------- leveldb


@dataclass(frozen=True, slots=True)
class Entry:
    key: bytes
    seq: int
    kind: int
    value: bytes | None
    source: str


@dataclass(frozen=True, slots=True)
class Record:
    """One LevelDB entry with its forensic status.

    live        newest entry for the key, holding a value
    superseded  older value; the key was later overwritten and is still live
    deleted     older value of a key whose newest entry is a deletion
    tombstone   a deletion marker (no value)
    """

    key: bytes
    seq: int
    value: bytes | None
    source: str
    status: str


STATUSES = ("live", "deleted", "superseded", "tombstone")


def iter_block(block: bytes) -> Iterator[tuple[bytes, bytes]]:
    if len(block) < 4:
        raise ValueError("short block")
    nrest = struct.unpack_from("<I", block, len(block) - 4)[0]
    limit = len(block) - 4 - 4 * nrest
    if limit < 0:
        raise ValueError("bad restart count")
    pos, key = 0, b""
    while pos < limit:
        shared, pos = read_varint(block, pos)
        non_shared, pos = read_varint(block, pos)
        vlen, pos = read_varint(block, pos)
        key = key[:shared] + block[pos : pos + non_shared]
        pos += non_shared
        yield key, block[pos : pos + vlen]
        pos += vlen


def read_block(data: bytes, offset: int, size: int) -> bytes:
    end = offset + size
    if end + BLOCK_TRAILER > len(data):
        raise ValueError("block past EOF")
    compression = data[end]
    if compression == 0:
        return data[offset:end]
    if compression == 1:
        return snappy_decompress(data[offset:end])
    raise ValueError(f"unsupported block compression {compression}")


def iter_table(data: bytes, source: str) -> Iterator[Entry]:
    if len(data) < TABLE_FOOTER or data[-8:] != TABLE_MAGIC:
        raise ValueError("not a LevelDB table (bad footer magic)")
    footer = data[-TABLE_FOOTER:]
    _, p = read_varint(footer, 0)  # metaindex handle (unused)
    _, p = read_varint(footer, p)
    index_offset, p = read_varint(footer, p)
    index_size, _ = read_varint(footer, p)
    for _, handle in iter_block(read_block(data, index_offset, index_size)):
        offset, q = read_varint(handle, 0)
        size, _ = read_varint(handle, q)
        for internal_key, value in iter_block(read_block(data, offset, size)):
            if len(internal_key) < 8:
                raise ValueError("short internal key")
            trailer = struct.unpack("<Q", internal_key[-8:])[0]
            kind = trailer & 0xFF
            yield Entry(
                internal_key[:-8], trailer >> 8, kind, value if kind == TYPE_VALUE else None, source
            )


def iter_log_records(data: bytes, warnings: list[str], name: str) -> Iterator[bytes]:
    pos, n = 0, len(data)
    fragment: bytearray | None = None
    while pos < n:
        left = LOG_BLOCK - pos % LOG_BLOCK
        if left < LOG_HEADER:  # block trailer padding
            pos += left
            continue
        if pos + LOG_HEADER > n:
            break
        length, record_type = struct.unpack_from("<HB", data, pos + 4)
        start = pos + LOG_HEADER
        end = start + length
        if record_type == 0 and length == 0:  # preallocated zeros
            pos = (pos // LOG_BLOCK + 1) * LOG_BLOCK
            continue
        if end > n:
            warnings.append(f"{name}: truncated tail record (normal for a live log)")
            break
        payload, pos = data[start:end], end
        if record_type == 1:  # FULL
            fragment = None
            yield payload
        elif record_type == 2:  # FIRST
            fragment = bytearray(payload)
        elif record_type == 3 and fragment is not None:  # MIDDLE
            fragment += payload
        elif record_type == 4 and fragment is not None:  # LAST
            fragment += payload
            yield bytes(fragment)
            fragment = None


def iter_batch(batch: bytes, source: str) -> Iterator[Entry]:
    if len(batch) < 12:
        raise ValueError("short write batch")
    seq, count = struct.unpack_from("<QI", batch, 0)
    pos = 12
    for i in range(count):
        tag = batch[pos]
        pos += 1
        key, pos = read_slice(batch, pos)
        value: bytes | None = None
        if tag == TYPE_VALUE:
            value, pos = read_slice(batch, pos)
        elif tag != TYPE_DELETION:
            raise ValueError(f"bad write-batch tag {tag}")
        yield Entry(key, seq + i, tag, value, source)


def file_entries(path: Path, warnings: list[str]) -> Iterator[Entry]:
    data = path.read_bytes()
    if path.suffix == ".log":
        for batch in iter_log_records(data, warnings, path.name):
            yield from iter_batch(batch, path.name)
    else:
        yield from iter_table(data, path.name)


def classify(entry: Entry, newest: Entry) -> str:
    if entry.kind != TYPE_VALUE or entry.value is None:
        return "tombstone"
    if entry is newest:
        return "live"
    return "deleted" if newest.kind != TYPE_VALUE else "superseded"


def load_leveldb(root: Path, warnings: list[str]) -> tuple[list[Record], list[str]]:
    """Read every surviving entry from tables + logs and classify it.

    Only history LevelDB has not yet compacted away is recoverable; absence of
    history is not evidence that a record was never changed or deleted.
    """
    entries: dict[tuple[bytes, int], Entry] = {}  # a compacted entry can sit in 2 files
    files = sorted(
        p
        for p in root.iterdir()
        if p.is_file() and not p.is_symlink() and p.suffix in {".ldb", ".sst", ".log"}
    )
    for path in files:
        try:
            for entry in file_entries(path, warnings):
                entries.setdefault((entry.key, entry.seq), entry)
        except (ValueError, struct.error, IndexError) as exc:
            warnings.append(f"{path.name}: {exc} (records before this point kept)")
    newest: dict[bytes, Entry] = {}
    for entry in entries.values():
        if entry.key not in newest or entry.seq > newest[entry.key].seq:
            newest[entry.key] = entry
    records = [
        Record(e.key, e.seq, e.value, e.source, classify(e, newest[e.key]))
        for e in entries.values()
    ]
    records.sort(key=lambda r: (r.key, -r.seq))  # per key: newest first
    return records, [p.name for p in files]


# ---------------------------------------------------------------- indexeddb


def decode_prefix(key: bytes) -> tuple[Json | None, bytes]:
    """Chromium IndexedDB KeyPrefix: 1 length byte + LE db/store/index ids."""
    if not key:
        return None, key
    lengths = key[0]
    db_len = (lengths >> 5) + 1
    store_len = ((lengths >> 2) & 7) + 1
    index_len = (lengths & 3) + 1
    need = 1 + db_len + store_len + index_len
    if len(key) < need:
        return None, key
    p = 1
    db = int.from_bytes(key[p : p + db_len], "little")
    p += db_len
    store = int.from_bytes(key[p : p + store_len], "little")
    p += store_len
    index = int.from_bytes(key[p : p + index_len], "little")
    if db == 0:
        kind = "global_metadata"
    elif store == 0:
        kind = "database_metadata"
    else:
        fixed = {1: "object_store_data", 2: "exists_entry", 3: "blob_entry"}
        kind = fixed.get(index, f"index:{index}" if index >= 30 else f"reserved:{index}")
    prefix = {"database_id": db, "object_store_id": store, "index_id": index, "kind": kind}
    return prefix, key[need:]


def printable_runs(data: bytes, minlen: int = 4, limit: int = 12) -> list[str]:
    """Heuristic: ASCII runs, which catches V8 one-byte strings in values.

    Each run is clipped to PREVIEW_CHARS so metadata size never tracks value
    size; the exact bytes are in records.jsonl.
    """
    runs: list[str] = []
    current = bytearray()
    for c in data + b"\x00":
        if 32 <= c < 127:
            current.append(c)
            continue
        if len(current) >= minlen:
            runs.append(clip(current.decode("ascii"), PREVIEW_CHARS))
            if len(runs) >= limit:
                break
        current = bytearray()
    return runs


def clip(text: str, n: int) -> str:
    return text if len(text) <= n else text[: n - 1] + "…"


def show(data: bytes, n: int = KEY_CHARS) -> str:
    if data and all(32 <= c < 127 for c in data):
        return clip(data.decode("ascii"), n)
    return clip("0x" + data.hex(), n)


# ---------------------------------------------------------------- blobs

MAGIC = (
    (b"\x89PNG\r\n\x1a\n", "image/png"),
    (b"\xff\xd8\xff", "image/jpeg"),
    (b"GIF8", "image/gif"),
    (b"%PDF-", "application/pdf"),
    (b"PK\x03\x04", "application/zip"),
    (b"\x1f\x8b", "application/gzip"),
    (b"SQLite format 3\x00", "application/x-sqlite3"),
    (b"\x1aE\xdf\xa3", "video/webm"),
    (b"OggS", "audio/ogg"),
    (b"ID3", "audio/mpeg"),
)


def sniff(head: bytes) -> str:
    for signature, mime in MAGIC:
        if head.startswith(signature):
            return mime
    if head[:4] == b"RIFF" and head[8:12] == b"WEBP":
        return "image/webp"
    if head[4:8] == b"ftyp":
        return "video/mp4"
    if b"\x00" not in head:
        try:
            head.decode("utf-8")
        except UnicodeDecodeError:
            pass
        else:
            return "text/plain"
    return "application/octet-stream"


def hash_file(path: Path) -> tuple[str, int, bytes]:
    digest, size, head = hashlib.sha256(), 0, b""
    with path.open("rb") as f:
        while chunk := f.read(CHUNK):
            if not head:
                head = chunk[:SNIFF_BYTES]
            digest.update(chunk)
            size += len(chunk)
    return digest.hexdigest(), size, head


def blob_ids(parts: tuple[str, ...]) -> tuple[int | None, int | None]:
    """Chromium layout: <database id hex>/<blob number byte hex>/<blob number hex>."""
    if len(parts) != 3:
        return None, None
    try:
        return int(parts[0], 16), int(parts[2], 16)
    except ValueError:
        return None, None


def scan_blobs(root: Path, dest: Path, max_bytes: int, warnings: list[str]) -> list[Json]:
    blobs: list[Json] = []
    for dirpath, dirnames, filenames in os.walk(root, followlinks=False):
        dirnames.sort()
        for name in sorted(filenames):
            path = Path(dirpath) / name
            rel = path.relative_to(root)
            if path.is_symlink() or not path.is_file():
                warnings.append(f"blob {rel}: skipped (symlink or special file)")
                continue
            digest, size, head = hash_file(path)
            database_id, blob_number = blob_ids(rel.parts)
            copied = size <= max_bytes
            if copied and not (dest / digest).exists():  # dedup identical blobs
                shutil.copyfile(path, dest / digest)
            blobs.append(
                {
                    "path": rel.as_posix(),
                    "size": size,
                    "sha256": digest,
                    "mime_guess": sniff(head),
                    "database_id": database_id,
                    "blob_number": blob_number,
                    "copied_as": f"blobs/{digest}" if copied else None,
                }
            )
    return blobs


# ---------------------------------------------------------------- views


def record_meta(n: int, record: Record) -> Json:
    prefix, user_key = decode_prefix(record.key)
    value = record.value
    return {
        "id": n,
        "status": record.status,
        "key_display": show(user_key),
        "key_sha256": hashlib.sha256(record.key).hexdigest(),
        "prefix": prefix,
        "sequence": record.seq,
        "source_file": record.source,
        "value_sha256": hashlib.sha256(value).hexdigest() if value is not None else None,
        "value_size": len(value) if value is not None else 0,
        "value_strings": printable_runs(value) if value is not None else [],
    }


def summarize(
    files: list[str], records: list[Json], blobs: list[Json], warnings: list[str]
) -> Json:
    live = [r for r in records if r["status"] == "live"]
    kinds = Counter((r["prefix"] or {}).get("kind", "unrecognised key") for r in live)
    statuses = Counter(r["status"] for r in records)
    return {
        "leveldb_files": len(files),
        "entries": len(records),
        "records": len(live),
        "status": {s: statuses.get(s, 0) for s in STATUSES},
        "value_bytes": sum(r["value_size"] for r in live),
        "kinds": dict(sorted(kinds.items())),
        "blobs": len(blobs),
        "blob_bytes": sum(b["size"] for b in blobs),
        "blob_types": dict(sorted(Counter(b["mime_guess"] for b in blobs).items())),
        "blobs_hashed_only": sum(1 for b in blobs if b["copied_as"] is None),
        "warnings": len(warnings),
    }


def history_order(r: Json) -> tuple[int, str, int]:
    """Deleted values first: they are what a live-state view loses."""
    return STATUSES.index(r["status"]), r["key_sha256"], -r["sequence"]


def dashboard_row(r: Json) -> Json:
    prefix = r["prefix"] or {}
    return {
        "status": r["status"],
        "sequence": r["sequence"],
        "kind": prefix.get("kind", "unrecognised key"),
        "database_id": prefix.get("database_id"),
        "object_store_id": prefix.get("object_store_id"),
        "key": r["key_display"],
        "size": r["value_size"],
        "strings": [clip(s, PREVIEW_CHARS) for s in r["value_strings"][:PREVIEW_STRINGS]],
    }


def dashboard_view(doc: Json, rows: int) -> Json:
    """Size-bounded projection: totals plus at most `rows` per table."""
    live = [r for r in doc["records"] if r["status"] == "live"]
    history = sorted((r for r in doc["records"] if r["status"] != "live"), key=history_order)
    records = [dashboard_row(r) for r in live[:rows]]
    blobs = [
        {
            "path": b["path"],
            "size": b["size"],
            "mime": b["mime_guess"],
            "database_id": b["database_id"],
            "sha256": b["sha256"],
            "copied": b["copied_as"] is not None,
        }
        for b in doc["blobs"][:rows]
    ]
    return {
        "tool": TOOL,
        "format_version": FORMAT_VERSION,
        "view": "dashboard",
        "summary": doc["summary"],
        "records": records,
        "history": [dashboard_row(r) for r in history[:rows]],
        "blobs": blobs,
        "warnings": [clip(w, 300) for w in doc["warnings"][:MAX_WARNINGS]],
    }


def table(headers: list[str], rows: list[list[Any]]) -> str:
    widths = [max(len(str(x)) for x in col) for col in zip(headers, *rows, strict=False)]

    def line(r: list[Any]) -> str:
        return "  ".join(str(x).ljust(w) for x, w in zip(r, widths, strict=True)).rstrip()

    return "\n".join([line(headers), line(["-" * w for w in widths]), *map(line, rows)])


RECORD_HEADERS = ["status", "seq", "kind", "db", "store", "key", "bytes", "strings"]


def record_row(r: Json) -> list[Any]:
    prefix = r["prefix"] or {}
    return [
        r["status"],
        r["sequence"],
        prefix.get("kind", "?"),
        prefix.get("database_id", ""),
        prefix.get("object_store_id", ""),
        r["key_display"],
        r["value_size"],
        clip(", ".join(r["value_strings"][:3]), 48),
    ]


def record_section(title: str, records: list[Json], rows: int) -> list[str]:
    if not records:
        return []
    out = [title, table(RECORD_HEADERS, [record_row(r) for r in records[:rows]])]
    if len(records) > rows:
        out.append(f"… first {rows} of {len(records)}; all entries are in export.json")
    return [*out, ""]


def render_report(doc: Json, rows: int) -> str:
    s = doc["summary"]
    status = ", ".join(f"{k} {n}" for k, n in s["status"].items())
    out = [
        f"{TOOL} v{FORMAT_VERSION}",
        f"leveldb files : {s['leveldb_files']}",
        f"live records  : {s['records']} ({s['value_bytes']} value bytes)",
        f"entries       : {s['entries']} ({status})",
        f"blobs         : {s['blobs']} ({s['blob_bytes']} bytes, "
        f"{s['blobs_hashed_only']} hashed only)",
        f"warnings      : {s['warnings']}",
        "",
    ]
    live = [r for r in doc["records"] if r["status"] == "live"]
    history = sorted((r for r in doc["records"] if r["status"] != "live"), key=history_order)
    out += record_section("RECORDS", live, rows)
    out += record_section("HISTORY (uncompacted; absence proves nothing)", history, rows)
    blob_rows = [
        [
            b["path"],
            b["size"],
            b["mime_guess"],
            b["sha256"][:12],
            "yes" if b["copied_as"] else "no (too big)",
        ]
        for b in doc["blobs"][:rows]
    ]
    if blob_rows:
        out += ["BLOBS", table(["path", "bytes", "type", "sha256", "copied"], blob_rows)]
        if s["blobs"] > rows:
            out.append(f"… first {rows} of {s['blobs']}; all blobs are in export.json")
        out.append("")
    if doc["warnings"]:
        out += ["WARNINGS", *(f"! {w}" for w in doc["warnings"]), ""]
    return "\n".join(out)


# ---------------------------------------------------------------- output


def dump(obj: Json, *, pretty: bool) -> str:
    if pretty:
        return json.dumps(obj, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    return json.dumps(obj, sort_keys=True, ensure_ascii=False, separators=(",", ":")) + "\n"


@dataclass(frozen=True, slots=True)
class Options:
    leveldb: Path
    blobs: Path | None
    max_blob_bytes: int
    preview_rows: int
    live_only: bool


def b64(data: bytes | None) -> str | None:
    return None if data is None else base64.b64encode(data).decode()


def build(tmp: Path, opts: Options) -> tuple[str, str]:
    """Write every output into `tmp`. Returns (export.json text, report text)."""
    warnings: list[str] = []
    records, files = load_leveldb(opts.leveldb, warnings)
    if opts.live_only:
        records = [r for r in records if r.status == "live"]
    (tmp / "blobs").mkdir()
    blobs = (
        scan_blobs(opts.blobs, tmp / "blobs", opts.max_blob_bytes, warnings) if opts.blobs else []
    )
    with (tmp / "records.jsonl").open("w", encoding="utf-8") as jsonl:
        for n, record in enumerate(records):
            line = {"id": n, "key_b64": b64(record.key), "value_b64": b64(record.value)}
            jsonl.write(dump(line, pretty=False))
    metas = [record_meta(n, r) for n, r in enumerate(records)]
    doc: Json = {
        "tool": TOOL,
        "format_version": FORMAT_VERSION,
        "view": "full",
        "summary": summarize(files, metas, blobs, warnings),
        "leveldb_files": files,
        "records": metas,
        "blobs": blobs,
        "warnings": warnings,
    }
    text = dump(doc, pretty=True)
    (tmp / "export.json").write_text(text, encoding="utf-8")
    (tmp / "dashboard.json").write_text(
        dump(dashboard_view(doc, opts.preview_rows), pretty=False), encoding="utf-8"
    )
    report = render_report(doc, opts.preview_rows)
    (tmp / "report.txt").write_text(report + "\n", encoding="utf-8")
    (tmp / MARKER).write_text(f"{TOOL} {FORMAT_VERSION}\n", encoding="utf-8")
    return text, report


def same_file(a: Path, b: Path) -> bool:
    return a.is_file() and b.is_file() and a.read_bytes() == b.read_bytes()


def publish(out: Path, fn: Callable[[Path], tuple[str, str]]) -> tuple[tuple[str, str], bool]:
    """Build in a sibling temp dir, then swap in. Never touches foreign dirs."""
    if out.exists() and not (out / MARKER).is_file():
        raise SystemExit(f"refusing to replace {out}: exists and is not an {TOOL} export")
    out.parent.mkdir(parents=True, exist_ok=True)
    tmp = out.parent / f".{out.name}.tmp-{os.getpid()}"
    shutil.rmtree(tmp, ignore_errors=True)
    tmp.mkdir()
    try:
        result = fn(tmp)
    except BaseException:
        shutil.rmtree(tmp, ignore_errors=True)
        raise
    unchanged = all(same_file(out / n, tmp / n) for n in ("export.json", "records.jsonl"))
    if out.exists():
        old = out.parent / f".{out.name}.old-{os.getpid()}"
        out.rename(old)
        tmp.rename(out)
        shutil.rmtree(old)
    else:
        tmp.rename(out)
    return result, unchanged


def parse_args(argv: list[str] | None) -> tuple[Options, Path, bool]:
    p = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    p.add_argument("--leveldb", required=True, type=Path, help="the *.indexeddb.leveldb directory")
    p.add_argument("--blobs", type=Path, help="the *.indexeddb.blob directory (optional)")
    p.add_argument("--out", required=True, type=Path, help="export directory")
    p.add_argument(
        "--max-blob-mib",
        type=int,
        default=64,
        help="copy blobs up to this size; larger ones are hashed only",
    )
    p.add_argument(
        "--preview-rows",
        type=int,
        default=DEFAULT_PREVIEW_ROWS,
        help="rows per table in dashboard.json and report.txt",
    )
    p.add_argument(
        "--live-only",
        action="store_true",
        help="omit superseded/deleted values and tombstones (default: keep them)",
    )
    p.add_argument("--json", action="store_true", help="print export.json instead of report")
    a = p.parse_args(argv)

    leveldb = a.leveldb.expanduser().resolve()
    blobs = a.blobs.expanduser().resolve() if a.blobs else None
    out = a.out.expanduser().resolve()
    for root in filter(None, (leveldb, blobs)):
        if not root.is_dir():
            p.error(f"input directory not found: {root}")
        if out == root or root in out.parents or out in root.parents:
            p.error("output and input directories must not overlap")
    if a.max_blob_mib < 1:
        p.error("--max-blob-mib must be positive")
    if a.preview_rows < 1:
        p.error("--preview-rows must be positive")
    opts = Options(leveldb, blobs, a.max_blob_mib << 20, a.preview_rows, a.live_only)
    return opts, out, a.json


def main(argv: list[str] | None = None) -> int:
    opts, out, as_json = parse_args(argv)
    (text, report), unchanged = publish(out, lambda tmp: build(tmp, opts))
    sys.stdout.write(text if as_json else report + "\n")
    print(f"→ {out} ({'unchanged' if unchanged else 'written'})", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
