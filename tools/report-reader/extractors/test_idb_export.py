"""Tests for idb_export against a synthetic LevelDB + blob store.

Run: python3 -I -m unittest discover -s extractors
"""

from __future__ import annotations

import base64
import io
import json
import struct
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import idb_export as ix

MAGIC_FOOTER = struct.pack("<Q", 0xDB4775248B80FB57)


def varint(n: int) -> bytes:
    out = bytearray()
    while n >= 0x80:
        out.append(n & 0x7F | 0x80)
        n >>= 7
    out.append(n)
    return bytes(out)


def prefix(db: int, store: int, index: int) -> bytes:
    return bytes([0, db, store, index])  # all ids 1 byte wide


def batch(seq: int, ops: list[tuple[bytes, bytes | None]]) -> bytes:
    out = struct.pack("<QI", seq, len(ops))
    for key, value in ops:
        if value is None:
            out += b"\x00" + varint(len(key)) + key
        else:
            out += b"\x01" + varint(len(key)) + key + varint(len(value)) + value
    return out


def log_record(payload: bytes) -> bytes:
    return struct.pack("<IHB", 0, len(payload), 1) + payload  # crc unchecked


def block(entries: list[tuple[bytes, bytes]]) -> bytes:
    out = b"".join(varint(0) + varint(len(k)) + varint(len(v)) + k + v for k, v in entries)
    return out + struct.pack("<II", 0, 1)


def snappy_literal(data: bytes) -> bytes:
    assert len(data) <= 256
    return varint(len(data)) + bytes([60 << 2, len(data) - 1]) + data


def table(entries: list[tuple[bytes, int, bytes]]) -> bytes:
    data = block([(k + struct.pack("<Q", seq << 8 | 1), v) for k, seq, v in entries])
    compressed = snappy_literal(data)
    out = compressed + b"\x01" + b"\0" * 4  # snappy block + trailer
    index = block([(b"\xff", varint(0) + varint(len(compressed)))])
    index_offset = len(out)
    out += index + b"\x00" + b"\0" * 4
    footer = varint(0) + varint(0) + varint(index_offset) + varint(len(index))
    return out + footer + b"\0" * (40 - len(footer)) + MAGIC_FOOTER


KEY_A = prefix(1, 1, 1) + b"\x01user-a"
KEY_B = prefix(1, 1, 1) + b"\x01user-b"
KEY_DB = prefix(1, 0, 0) + b"\x32dbname"


def make_fixture(root: Path, extra_records: int = 0) -> tuple[Path, Path]:
    leveldb, blobs = root / "db.leveldb", root / "db.blob"
    (blobs / "1" / "00").mkdir(parents=True)
    leveldb.mkdir()
    extra: list[tuple[bytes, bytes | None]] = [
        (prefix(1, 2, 1) + b"bulk-%05d" % i, b"payload-%05d" % i) for i in range(extra_records)
    ]
    log = log_record(batch(10, [(KEY_A, b'o"\x04nameS\x05Alice{'), (KEY_B, b"gone")]))
    log += log_record(batch(12, [(KEY_A, b'o"\x04nameS\x05Alice2{'), (KEY_B, None)]))
    if extra:
        log += log_record(batch(20, extra))
    (leveldb / "000003.log").write_bytes(log)
    (leveldb / "000005.ldb").write_bytes(
        table([(KEY_DB, 3, b"my-database"), (KEY_A, 5, b"stale-old-value")])
    )
    (leveldb / "LOG").write_text("not data")
    (blobs / "1" / "00" / "1").write_bytes(b"\x89PNG\r\n\x1a\n" + b"\0" * 100)
    (blobs / "1" / "00" / "2").write_bytes(b"hello blob\n")
    return leveldb, blobs


def run(*args: str) -> int:
    with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
        return ix.main(list(args))


class SnappyTest(unittest.TestCase):
    def test_literal_and_overlapping_copy(self) -> None:
        compressed = varint(12) + bytes([3 << 2]) + b"abcd" + bytes([1 | (4 << 2), 4])
        self.assertEqual(ix.snappy_decompress(compressed), b"abcdabcdabcd")

    def test_bad_offset_rejected(self) -> None:
        with self.assertRaises(ValueError):
            ix.snappy_decompress(varint(8) + bytes([1 | (4 << 2), 9]))


class ExportTest(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def export(self, extra: int = 0, *flags: str) -> Path:
        leveldb, blobs = make_fixture(self.root, extra)
        out = self.root / "out"
        self.assertEqual(
            run("--leveldb", str(leveldb), "--blobs", str(blobs), "--out", str(out), *flags), 0
        )
        return out

    def test_history_is_classified(self) -> None:
        doc = json.loads((self.export() / "export.json").read_text())
        status = {(r["key_display"], r["sequence"]): r["status"] for r in doc["records"]}
        self.assertEqual(status[("0x01757365722d61", 12)], "live")  # user-a newest
        self.assertEqual(status[("0x01757365722d61", 10)], "superseded")
        self.assertEqual(status[("0x01757365722d61", 5)], "superseded")  # from the table
        self.assertEqual(status[("0x01757365722d62", 11)], "deleted")  # user-b's value
        self.assertEqual(status[("0x01757365722d62", 13)], "tombstone")
        self.assertEqual(doc["summary"]["records"], 2)
        self.assertEqual(
            doc["summary"]["status"], {"live": 2, "deleted": 1, "superseded": 2, "tombstone": 1}
        )
        self.assertIn("stale-old-value", json.dumps(doc))

    def test_deleted_value_bytes_recoverable(self) -> None:
        out = self.export()
        meta = json.loads((out / "export.json").read_text())["records"]
        deleted = next(m["id"] for m in meta if m["status"] == "deleted")
        text = (out / "records.jsonl").read_text(encoding="utf-8")
        lines = {x["id"]: x for x in map(json.loads, text.splitlines())}
        self.assertEqual(base64.b64decode(lines[deleted]["value_b64"]), b"gone")
        tomb = next(m["id"] for m in meta if m["status"] == "tombstone")
        self.assertIsNone(lines[tomb]["value_b64"])

    def test_live_only(self) -> None:
        doc = json.loads((self.export(0, "--live-only") / "export.json").read_text())
        self.assertEqual({r["status"] for r in doc["records"]}, {"live"})
        self.assertNotIn("stale-old-value", json.dumps(doc))

    def test_full_bytes_only_in_jsonl(self) -> None:
        out = self.export()
        self.assertNotIn("_b64", (out / "export.json").read_text())
        self.assertNotIn("_b64", (out / "dashboard.json").read_text())
        lines = [json.loads(x) for x in (out / "records.jsonl").read_text().splitlines()]
        meta = json.loads((out / "export.json").read_text())["records"]
        self.assertEqual([x["id"] for x in lines], [m["id"] for m in meta])

    def test_idempotent(self) -> None:
        out = self.export()
        first = {p.name: p.read_bytes() for p in out.iterdir() if p.is_file()}
        self.assertEqual(
            run(
                "--leveldb",
                str(self.root / "db.leveldb"),
                "--blobs",
                str(self.root / "db.blob"),
                "--out",
                str(out),
            ),
            0,
        )
        second = {p.name: p.read_bytes() for p in out.iterdir() if p.is_file()}
        self.assertEqual(first, second)

    def test_dashboard_is_bounded(self) -> None:
        out = self.export(300, "--preview-rows", "25")
        dash = json.loads((out / "dashboard.json").read_text())
        self.assertEqual(dash["view"], "dashboard")
        self.assertEqual(len(dash["records"]), 25)
        self.assertEqual(dash["summary"]["records"], 302)
        self.assertEqual(dash["history"][0]["status"], "deleted")  # deleted values first
        self.assertEqual(dash["summary"]["kinds"]["object_store_data"], 301)

    def test_refuses_foreign_directory(self) -> None:
        leveldb, _ = make_fixture(self.root)
        foreign = self.root / "foreign"
        foreign.mkdir()
        with self.assertRaises(SystemExit):
            run("--leveldb", str(leveldb), "--out", str(foreign))


class PreviewBoundTest(unittest.TestCase):
    def test_value_strings_do_not_track_value_size(self) -> None:
        runs = ix.printable_runs(b"x" * 100_000 + b"\x00" + b"y" * 50)
        self.assertEqual([len(r) for r in runs], [ix.PREVIEW_CHARS, 50])  # long clipped


if __name__ == "__main__":
    unittest.main()
