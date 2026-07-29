// The backup-zip reader, tested as the pure function it is.
//
// `zipRead`'s output is written straight into the reader's home, so a damaged
// archive must fail by name — never quietly restore short or wrong bytes. These
// cases are plain async functions over `node:assert` (no Playwright, no page)
// so they can be run by anything that can import TypeScript; `zip.spec.ts`
// registers them so CI's Playwright run covers them.

import assert from "node:assert/strict";
import { zipRead, zipWrite } from "../src/engine/zip";

const enc = new TextEncoder();

const FILES = new Map<string, Uint8Array>([
  ["notes/", new Uint8Array(0)], // a directory entry: skipped on read
  ["notes/Gen.1.7.json", enc.encode(JSON.stringify({ format: "pure-note-v1", text: "waters" }))],
  ["tags/Ἀρχή.json", enc.encode('{"name":"Ἀρχή"}')],
  ["threads/empty.json", new Uint8Array(0)],
  ["weaves/big.json", enc.encode("x".repeat(4096))],
]);

/** Offsets inside the fixed-size records, so the patches below read as intent. */
const EOCD_COUNT = 10;
const EOCD_CD_OFFSET = 16;
const CEN_CSIZE = 20;
const CEN_USIZE = 24;
const CEN_NAME_LEN = 28;
const CEN_EXTRA_LEN = 30;
const CEN_LOCAL_OFFSET = 42;

/** A zip this module wrote, plus the offsets a test needs to corrupt it. */
function good(): { buf: Uint8Array; dv: DataView; eocd: number; cd: number } {
  const buf = zipWrite(FILES);
  const dv = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  const eocd = buf.length - 22; // zipWrite writes no archive comment
  assert.equal(dv.getUint32(eocd, true), 0x06054b50, "fixture: EOCD is the last 22 bytes");
  return { buf, dv, eocd, cd: dv.getUint32(eocd + EOCD_CD_OFFSET, true) };
}

/** Walk to the nth central-directory record. */
function cenAt(dv: DataView, cd: number, n: number): number {
  let at = cd;
  for (let i = 0; i < n; i++) {
    const nameLen = dv.getUint16(at + 28, true);
    at += 46 + nameLen + dv.getUint16(at + CEN_EXTRA_LEN, true) + dv.getUint16(at + 32, true);
  }
  assert.equal(dv.getUint32(at, true), 0x02014b50, "fixture: walked to a central-directory record");
  return at;
}

/** A raw-deflate STORED block — valid `deflate-raw`, byte-identical payload, so
 *  a flipped byte still inflates and only the CRC can catch it. */
function deflateStored(payload: Uint8Array): Uint8Array {
  const out = new Uint8Array(5 + payload.length);
  out[0] = 0x01; // BFINAL=1, BTYPE=00 (stored)
  out[1] = payload.length & 0xff;
  out[2] = (payload.length >> 8) & 0xff;
  out[3] = ~payload.length & 0xff;
  out[4] = (~payload.length >> 8) & 0xff;
  out.set(payload, 5);
  return out;
}

/** Hand-built single-entry zip, so the method-8 path can be driven directly.
 *  `descriptor` zeroes the local header's crc and sizes and appends a data
 *  descriptor — the shape java.util.zip.ZipOutputStream writes, which is what
 *  the Android backup is, and the reason the reader trusts only the central
 *  directory for sizes and the CRC. */
function oneEntryZip(o: {
  name: string;
  method: number;
  stored: Uint8Array;
  crc: number;
  usize: number;
  descriptor?: boolean;
}): Uint8Array {
  const nameB = enc.encode(o.name);
  const local = new DataView(new ArrayBuffer(30));
  local.setUint32(0, 0x04034b50, true);
  local.setUint16(4, 20, true);
  local.setUint16(6, o.descriptor ? 0x0808 : 0, true); // bit 3 descriptor, bit 11 UTF-8
  local.setUint16(8, o.method, true);
  local.setUint32(14, o.descriptor ? 0 : o.crc, true);
  local.setUint32(18, o.descriptor ? 0 : o.stored.length, true);
  local.setUint32(22, o.descriptor ? 0 : o.usize, true);
  local.setUint16(26, nameB.length, true);

  const desc = new DataView(new ArrayBuffer(o.descriptor ? 16 : 0));
  if (o.descriptor) {
    desc.setUint32(0, 0x08074b50, true);
    desc.setUint32(4, o.crc, true);
    desc.setUint32(8, o.stored.length, true);
    desc.setUint32(12, o.usize, true);
  }

  const cen = new DataView(new ArrayBuffer(46));
  cen.setUint32(0, 0x02014b50, true);
  cen.setUint16(4, 20, true);
  cen.setUint16(6, 20, true);
  cen.setUint16(8, o.descriptor ? 0x0808 : 0, true);
  cen.setUint16(10, o.method, true);
  cen.setUint32(16, o.crc, true);
  cen.setUint32(CEN_CSIZE, o.stored.length, true);
  cen.setUint32(CEN_USIZE, o.usize, true);
  cen.setUint16(28, nameB.length, true);
  cen.setUint32(CEN_LOCAL_OFFSET, 0, true);

  const cdAt = 30 + nameB.length + o.stored.length + desc.byteLength;
  const eocd = new DataView(new ArrayBuffer(22));
  eocd.setUint32(0, 0x06054b50, true);
  eocd.setUint16(8, 1, true);
  eocd.setUint16(EOCD_COUNT, 1, true);
  eocd.setUint32(12, 46 + nameB.length, true);
  eocd.setUint32(EOCD_CD_OFFSET, cdAt, true);

  const parts = [
    new Uint8Array(local.buffer),
    nameB,
    o.stored,
    new Uint8Array(desc.buffer),
    new Uint8Array(cen.buffer),
    nameB,
    new Uint8Array(eocd.buffer),
  ];
  const out = new Uint8Array(parts.reduce((s, p) => s + p.length, 0));
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

/** CRC-32 (IEEE) — the test's own copy, so it cannot agree with a broken one
 *  in zip.ts by sharing the bug. */
function refCrc32(bytes: Uint8Array): number {
  let c = 0xffffffff;
  for (let i = 0; i < bytes.length; i++) {
    c ^= bytes[i];
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  }
  return (c ^ 0xffffffff) >>> 0;
}

/** Assert the read is refused, and that the message would tell a reader which
 *  file went wrong (SettingsDialog shows it to them verbatim as a toast). */
async function refuses(buf: Uint8Array, wants: RegExp, why: string): Promise<void> {
  let msg: string | null = null;
  try {
    await zipRead(buf);
  } catch (e) {
    msg = e instanceof Error ? e.message : String(e);
  }
  if (msg === null) assert.fail(`${why}: zipRead RESTORED a damaged zip instead of refusing`);
  assert.match(msg, wants, `${why}: refused, but not with a message naming the trouble — got "${msg}"`);
}

export const zipCases: Array<{ name: string; run: () => Promise<void> }> = [
  {
    name: "a zip this module writes round-trips through it",
    run: async () => {
      const out = await zipRead(zipWrite(FILES));
      assert.deepEqual([...out.keys()], [...FILES.keys()].filter((k) => !k.endsWith("/")));
      for (const [path, bytes] of out) assert.deepEqual([...bytes], [...FILES.get(path)!], path);
    },
  },
  {
    name: "a deflated entry round-trips and its checksum is honoured",
    run: async () => {
      const raw = enc.encode('{"format":"overlay-weave-v2","links":[]}');
      const stored = deflateStored(raw);
      const buf = oneEntryZip({ name: "weaves/w.json", method: 8, stored, crc: refCrc32(raw), usize: raw.length });
      assert.deepEqual([...(await zipRead(buf)).get("weaves/w.json")!], [...raw]);
    },
  },
  {
    name: "an Android-shaped entry (data descriptor, empty local sizes) still reads",
    run: async () => {
      // ZipOutputStream leaves the local header's crc and sizes at zero and puts
      // the real values in a descriptor after the data. Take the sizes from the
      // local header instead and every Android backup reads as zero bytes.
      const raw = enc.encode('{"format":"pure-note-v1","text":"the waters"}');
      const out = await zipRead(
        oneEntryZip({
          name: "notes/Gen.1.7.json",
          method: 8,
          stored: deflateStored(raw),
          crc: refCrc32(raw),
          usize: raw.length,
          descriptor: true,
        }),
      );
      assert.deepEqual([...out.get("notes/Gen.1.7.json")!], [...raw]);
    },
  },
  {
    name: "a zip cut off at the tail is not a zip",
    run: async () => {
      const { buf } = good();
      // The EOCD is the last record, so any tail truncation loses it.
      await refuses(buf.subarray(0, buf.length - 1), /not a zip file/, "tail truncation");
    },
  },
  {
    name: "a file cut short is named, not silently restored short",
    run: async () => {
      // The last entry's data loses 3072 bytes; the EOCD is fixed up so the
      // central directory is still found — exactly what a bad copy looks like.
      const { buf, dv, cd } = good();
      const cut = 3072;
      assert.ok(cd > cut, "fixture: the splice stays inside the data region");
      assert.equal(dv.getUint32(cd, true), 0x02014b50, "fixture: cd points at the central directory");
      const spliced = new Uint8Array(buf.length - cut);
      spliced.set(buf.subarray(0, cd - cut));
      spliced.set(buf.subarray(cd), cd - cut);
      new DataView(spliced.buffer).setUint32(spliced.length - 22 + EOCD_CD_OFFSET, cd - cut, true);
      await refuses(spliced, /weaves\/big\.json is cut short/, "spliced-out data");
    },
  },
  {
    name: "a zip claiming a file longer than itself is named",
    run: async () => {
      const { buf, dv, cd } = good();
      dv.setUint32(cenAt(dv, cd, 1) + CEN_CSIZE, 0x00ffffff, true);
      dv.setUint32(cenAt(dv, cd, 1) + CEN_USIZE, 0x00ffffff, true);
      await refuses(buf, /notes\/Gen\.1\.7\.json is cut short/, "over-long compressed size");
    },
  },
  {
    name: "a flipped byte in a stored file fails its checksum",
    run: async () => {
      const { buf, dv, cd } = good();
      const localAt = dv.getUint32(cenAt(dv, cd, 1) + CEN_LOCAL_OFFSET, true);
      const dataAt = localAt + 30 + dv.getUint16(localAt + 26, true);
      buf[dataAt + 4] ^= 0x20; // still valid JSON-ish bytes; nothing else notices
      await refuses(buf, /notes\/Gen\.1\.7\.json doesn't match its checksum/, "flipped stored byte");
    },
  },
  {
    name: "a flipped byte inside deflated data fails its checksum",
    run: async () => {
      const name = "weaves/w.json";
      const raw = enc.encode('{"format":"overlay-weave-v2","links":[]}');
      const buf = oneEntryZip({ name, method: 8, stored: deflateStored(raw), crc: refCrc32(raw), usize: raw.length });
      // oneEntryZip puts the entry at offset 0, so the deflate stream starts
      // after the 30-byte local header and the name; +5 skips the stored-block
      // header into the literals, where a flip still inflates cleanly.
      buf[30 + name.length + 5 + 4] ^= 0x40;
      await refuses(buf, /weaves\/w\.json doesn't match its checksum/, "flipped deflate literal");
    },
  },
  {
    name: "a file that inflates to the wrong length is named",
    run: async () => {
      const raw = enc.encode("some study notes");
      const stored = deflateStored(raw);
      const buf = oneEntryZip({ name: "notes/n.json", method: 8, stored, crc: refCrc32(raw), usize: raw.length + 5 });
      await refuses(buf, /notes\/n\.json is the wrong length/, "wrong uncompressed size");
    },
  },
  {
    name: "deflate that cannot be unpacked at all is named",
    run: async () => {
      const junk = new Uint8Array([0xff, 0x07, 0x00, 0x00, 0x00]); // BTYPE=11, reserved
      const buf = oneEntryZip({ name: "notes/n.json", method: 8, stored: junk, crc: 0, usize: 4 });
      await refuses(buf, /notes\/n\.json could not be unpacked/, "undecodable deflate stream");
    },
  },
  {
    name: "a central directory pointing past the end is refused, not a RangeError",
    run: async () => {
      const { buf, dv, eocd } = good();
      dv.setUint32(eocd + EOCD_CD_OFFSET, buf.length + 4096, true);
      await refuses(buf, /file list runs past the end/, "central directory past the end");
    },
  },
  {
    name: "an entry whose name overruns the end is refused, not renamed",
    run: async () => {
      // `subarray` clamps, so an over-long name would otherwise be read as the
      // real name plus whatever follows it — a file restored under a garbage
      // path instead of a refusal.
      const { buf, dv, cd } = good();
      dv.setUint16(cenAt(dv, cd, 4) + CEN_NAME_LEN, 0xffff, true);
      await refuses(buf, /file list runs past the end/, "over-long name");
    },
  },
  {
    name: "more entries claimed than the zip holds is refused",
    run: async () => {
      const { buf, dv, eocd } = good();
      dv.setUint16(eocd + EOCD_COUNT, 99, true);
      await refuses(buf, /file list runs past the end/, "inflated entry count");
    },
  },
  {
    name: "a local-header offset past the end is named",
    run: async () => {
      const { buf, dv, cd } = good();
      dv.setUint32(cenAt(dv, cd, 2) + CEN_LOCAL_OFFSET, buf.length, true);
      await refuses(buf, /is cut short/, "local header past the end");
    },
  },
  {
    name: "a local-header offset pointing at something else is named",
    run: async () => {
      const { buf, dv, cd } = good();
      dv.setUint32(cenAt(dv, cd, 2) + CEN_LOCAL_OFFSET, 7, true);
      await refuses(buf, /isn't where the zip says it is/, "local offset off by seven");
    },
  },
];
