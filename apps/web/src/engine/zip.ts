// Minimal ZIP for study-data backups — no dependencies. Writes store-only
// archives (the files are small JSON; portability beats bytes) and reads
// store or deflate entries (deflate via DecompressionStream). The archive
// layout is the home's authored dirs, shared with the Android backup, so one
// zip restores across devices. Reading is checked, not trusted: every offset is
// bounds-checked and every entry's CRC-32 must match the central directory,
// because whatever `zipRead` returns is written into the reader's home.

const LOCAL_SIG = 0x04034b50;
const CENTRAL_SIG = 0x02014b50;
const EOCD_SIG = 0x06054b50;

const enc = new TextEncoder();

// CRC-32 (IEEE), table-driven.
const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();
function crc32(bytes: Uint8Array): number {
  let c = 0xffffffff;
  for (let i = 0; i < bytes.length; i++) c = CRC_TABLE[(c ^ bytes[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

/** Build a store-only zip from path → bytes. */
export function zipWrite(files: Map<string, Uint8Array>): Uint8Array {
  const chunks: Uint8Array[] = [];
  const central: Uint8Array[] = [];
  let offset = 0;

  for (const [path, data] of files) {
    const name = enc.encode(path);
    const crc = crc32(data);

    const local = new DataView(new ArrayBuffer(30));
    local.setUint32(0, LOCAL_SIG, true);
    local.setUint16(4, 20, true); // version needed
    local.setUint16(8, 0, true); // method: store
    local.setUint32(14, crc, true);
    local.setUint32(18, data.length, true);
    local.setUint32(22, data.length, true);
    local.setUint16(26, name.length, true);
    chunks.push(new Uint8Array(local.buffer), name, data);

    const cen = new DataView(new ArrayBuffer(46));
    cen.setUint32(0, CENTRAL_SIG, true);
    cen.setUint16(4, 20, true);
    cen.setUint16(6, 20, true);
    cen.setUint16(10, 0, true);
    cen.setUint32(16, crc, true);
    cen.setUint32(20, data.length, true);
    cen.setUint32(24, data.length, true);
    cen.setUint16(28, name.length, true);
    cen.setUint32(42, offset, true);
    central.push(new Uint8Array(cen.buffer), name);

    offset += 30 + name.length + data.length;
  }

  const centralSize = central.reduce((s, c) => s + c.length, 0);
  const eocd = new DataView(new ArrayBuffer(22));
  eocd.setUint32(0, EOCD_SIG, true);
  eocd.setUint16(8, files.size, true);
  eocd.setUint16(10, files.size, true);
  eocd.setUint32(12, centralSize, true);
  eocd.setUint32(16, offset, true);

  const total = offset + centralSize + 22;
  const out = new Uint8Array(total);
  let at = 0;
  for (const c of [...chunks, ...central, new Uint8Array(eocd.buffer)]) {
    out.set(c, at);
    at += c.length;
  }
  return out;
}

async function inflateRaw(data: Uint8Array, name: string): Promise<Uint8Array> {
  const stream = new Blob([data as unknown as BlobPart]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
  try {
    return new Uint8Array(await new Response(stream).arrayBuffer());
  } catch {
    throw new Error(`damaged backup: ${name} could not be unpacked`);
  }
}

/** Read a zip into path → bytes (store + deflate entries). */
export async function zipRead(buf: Uint8Array): Promise<Map<string, Uint8Array>> {
  const dv = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  // Find EOCD (scan back past any comment).
  let eocd = -1;
  for (let i = buf.length - 22; i >= Math.max(0, buf.length - 22 - 65536); i--) {
    if (dv.getUint32(i, true) === EOCD_SIG) {
      eocd = i;
      break;
    }
  }
  if (eocd < 0) throw new Error("not a zip file");
  const count = dv.getUint16(eocd + 10, true);
  let at = dv.getUint32(eocd + 16, true);

  // Restore writes these bytes straight into the reader's home, so nothing the
  // archive claims about itself is taken on trust. Every offset is checked
  // before it is read — `subarray` CLAMPS, so an over-long length would
  // otherwise hand back a silently short file instead of failing — and every
  // entry's CRC-32 is checked against the central directory, so a flipped byte
  // is a refusal by name rather than a bad restore.
  const fits = (start: number, len: number, what: string): void => {
    if (start < 0 || len < 0 || start + len > buf.length) throw new Error(`damaged backup: ${what}`);
  };

  const dec = new TextDecoder();
  const out = new Map<string, Uint8Array>();
  for (let n = 0; n < count; n++) {
    fits(at, 46, "the file list runs past the end of the zip");
    if (dv.getUint32(at, true) !== CENTRAL_SIG) throw new Error("bad central directory");
    const method = dv.getUint16(at + 10, true);
    const crc = dv.getUint32(at + 16, true);
    const csize = dv.getUint32(at + 20, true);
    const usize = dv.getUint32(at + 24, true);
    const nameLen = dv.getUint16(at + 28, true);
    const extraLen = dv.getUint16(at + 30, true);
    const commentLen = dv.getUint16(at + 32, true);
    const localAt = dv.getUint32(at + 42, true);
    fits(at + 46, nameLen + extraLen + commentLen, "the file list runs past the end of the zip");
    const name = dec.decode(buf.subarray(at + 46, at + 46 + nameLen));

    // The local header's name/extra lengths can differ from the central
    // directory's, so the data offset comes from the local header — but the
    // sizes and the CRC come from the central directory, which is the copy a
    // streamed zip (data descriptors) fills in correctly.
    fits(localAt, 30, `${name} is cut short`);
    if (dv.getUint32(localAt, true) !== LOCAL_SIG)
      throw new Error(`damaged backup: ${name} isn't where the zip says it is`);
    const lNameLen = dv.getUint16(localAt + 26, true);
    const lExtraLen = dv.getUint16(localAt + 28, true);
    const dataAt = localAt + 30 + lNameLen + lExtraLen;
    fits(dataAt, csize, `${name} is cut short`);
    const data = buf.subarray(dataAt, dataAt + csize);
    if (!name.endsWith("/")) {
      let bytes: Uint8Array;
      if (method === 0) bytes = data.slice();
      else if (method === 8) bytes = await inflateRaw(data, name);
      else throw new Error(`unsupported compression method ${method} for ${name}`);
      if (bytes.length !== usize) throw new Error(`damaged backup: ${name} is the wrong length`);
      if (crc32(bytes) !== crc) throw new Error(`damaged backup: ${name} doesn't match its checksum`);
      out.set(name, bytes);
    }
    at += 46 + nameLen + extraLen + commentLen;
  }
  return out;
}
