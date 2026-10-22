// The encrypted backup's engine calls: seal a zip under the phrase, open one, make
// a phrase. Runs in the engine worker, where the wasm instance lives, and where
// `crypto.getRandomValues` supplies the entropy the core asks for — the core
// generates none itself, so what it is fed is decided here and nowhere else.
//
// The file is age (age-encryption.org/v1): a reader with the `age` command and
// their phrase can open it without Plumbline. The format and the phrase live in
// crates/core/src/backup.rs; this is the byte-passing.

import type { WasmEngine } from "./engine";

export type BackupErrorKind = "entropy" | "notEncrypted" | "wrongPhrase" | "damaged";

/** Why a seal or open failed — `wrongPhrase` is the one to ask again on. */
export interface BackupFailure {
  kind: BackupErrorKind;
  message: string;
}

/** The engine's answer, as a value rather than a throw: a rejection crossing the
 *  worker boundary keeps only its message, and the shell needs the kind. */
export type BackupResult = { bytes: Uint8Array; error?: undefined } | { bytes?: undefined; error: BackupFailure };

/** What `plumbline_backup_encrypt` / `plumbline_backup_phrase` need from the CSPRNG. */
const ENTROPY_LEN = 48;
const PHRASE_ENTROPY_LEN = 8;

/** The first bytes of every age file. */
const AGE_MAGIC = new TextEncoder().encode("age-encryption.org/v1\n");

/** Whether `bytes` are an encrypted backup rather than a plain zip. */
export function isEncryptedBackup(bytes: Uint8Array): boolean {
  if (bytes.length < AGE_MAGIC.length) return false;
  for (let i = 0; i < AGE_MAGIC.length; i++) if (bytes[i] !== AGE_MAGIC[i]) return false;
  return true;
}

// Views are made per access: a growth detaches earlier buffers, and copying a
// zip in can grow the memory.
function alloc(w: WasmEngine, n: number): number {
  return n === 0 ? 0 : ((w.exports.plumbline_web_alloc as (n: number) => number)(n) as number);
}
function putBytes(w: WasmEngine, data: Uint8Array): number {
  const p = alloc(w, data.length);
  if (p) new Uint8Array(w.exports.memory.buffer).set(data, p);
  return p;
}
function dropBytes(w: WasmEngine, p: number, n: number): void {
  if (p) (w.exports.plumbline_web_free as (p: number, n: number) => void)(p, n);
}
/** Copy a returned buffer out and free the engine's. */
function takeBytes(w: WasmEngine, p: number, n: number): Uint8Array {
  const out = new Uint8Array(w.exports.memory.buffer, p, n).slice();
  (w.exports.plumbline_bytes_free as (p: number, n: number) => void)(p, n);
  return out;
}

/** Run one of the byte-returning calls: `f` gets the `out_len` and `out_err`
 *  slots and answers the buffer pointer. */
function call(w: WasmEngine, f: (lenSlot: number, errSlot: number) => number): BackupResult {
  const lenSlot = alloc(w, 4);
  try {
    const [ptr, err] = w.withErrSlot((errSlot) => f(lenSlot, errSlot));
    if (!ptr) {
      let error: BackupFailure = { kind: "damaged", message: "the engine gave no answer" };
      try {
        if (err) error = JSON.parse(err);
      } catch {
        error = { kind: "damaged", message: err ?? error.message };
      }
      return { error };
    }
    const len = new DataView(w.exports.memory.buffer).getUint32(lenSlot, true);
    return { bytes: takeBytes(w, ptr, len) };
  } finally {
    dropBytes(w, lenSlot, 4);
  }
}

/** Seal `zip` under `phrase`. Fresh entropy every call — the file key and the
 *  nonce must never repeat. */
export function backupSeal(w: WasmEngine, zip: Uint8Array, phrase: string): BackupResult {
  const entropy = crypto.getRandomValues(new Uint8Array(ENTROPY_LEN));
  const d = putBytes(w, zip);
  const e = putBytes(w, entropy);
  const p = w.inStr(phrase);
  try {
    return call(w, (lenSlot, errSlot) =>
      (w.exports.plumbline_backup_encrypt as Function)(d, zip.length, p, e, entropy.length, lenSlot, errSlot),
    );
  } finally {
    w.freeStr(p);
    dropBytes(w, e, entropy.length);
    dropBytes(w, d, zip.length);
  }
}

/** Open `file` with `phrase`: the zip inside, or why not. */
export function backupOpen(w: WasmEngine, file: Uint8Array, phrase: string): BackupResult {
  const d = putBytes(w, file);
  const p = w.inStr(phrase);
  try {
    return call(w, (lenSlot, errSlot) =>
      (w.exports.plumbline_backup_decrypt as Function)(d, file.length, p, lenSlot, errSlot),
    );
  } finally {
    w.freeStr(p);
    dropBytes(w, d, file.length);
  }
}

/** Merge a backup's files into the device's (core `merge`). Both sides cross as
 *  text: every file under the user dirs is JSON. Answers what to write over the
 *  device and what to delete from it. Throws when the engine refuses the
 *  request, which leaves the device untouched. */
export function backupMerge(
  w: WasmEngine,
  local: Map<string, Uint8Array>,
  incoming: Map<string, Uint8Array>,
): { write: [string, Uint8Array][]; delete: string[] } {
  const dec = new TextDecoder();
  const text = (m: Map<string, Uint8Array>) => Object.fromEntries([...m].map(([p, b]) => [p, dec.decode(b)]));
  const p = w.inStr(JSON.stringify({ local: text(local), incoming: text(incoming) }));
  try {
    const s = w.takeStr((w.exports.plumbline_backup_merge_json as Function)(p) as number);
    if (!s) throw new Error("the engine could not merge this backup");
    const r = JSON.parse(s) as { write: Record<string, string>; delete: string[] };
    const enc = new TextEncoder();
    return { write: Object.entries(r.write).map(([k, v]) => [k, enc.encode(v)]), delete: r.delete };
  } finally {
    w.freeStr(p);
  }
}

/** Five words for a new phrase. */
export function backupPhrase(w: WasmEngine): string {
  const entropy = crypto.getRandomValues(new Uint8Array(PHRASE_ENTROPY_LEN));
  const e = putBytes(w, entropy);
  try {
    const s = w.takeStr((w.exports.plumbline_backup_phrase as Function)(e, entropy.length) as number);
    if (!s) throw new Error("the engine made no phrase");
    return s;
  } finally {
    dropBytes(w, e, entropy.length);
  }
}
