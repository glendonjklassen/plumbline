// Instantiate the pure-ffi wasm module under the browser WASI shim and expose
// the raw C ABI plus string marshalling. The higher-level, method-for-method
// binding lives in StudyEngine.ts (the TS sibling of StudyEngine.kt /
// PureStudy.cs); this module owns the runtime plumbing only.

import {
  ConsoleStdout,
  File,
  OpenFile,
  PreopenDirectory,
  WASI,
  type Directory,
} from "@bjorn3/browser_wasi_shim";

export interface WasmEngine {
  exports: Record<string, Function> & { memory: WebAssembly.Memory };
  /** Copy a JS string in as a NUL-terminated UTF-8 C string. */
  inStr(s: string): number;
  /** Free a buffer from inStr. */
  freeStr(ptr: number): void;
  /** Read a returned C string, free it engine-side, and hand back the JS copy. */
  takeStr(ptr: number): string | null;
  /** Borrow a 4-byte slot for `char **out_err` params. */
  withErrSlot<T>(f: (slot: number) => T): [T, string | null];
  /** Point layout text measurement at the shell's current font. */
  setMeasure(measure: (text: string) => number): void;
  /** The measure callback as a PureMeasureFn value for pure_layout_* calls. */
  measureFnptr: number;
}

const enc = new TextEncoder();
const dec = new TextDecoder();

export async function instantiate(homeRoot: Map<string, Directory | File>): Promise<WasmEngine> {
  // fds 0–2 are stdio (panic messages surface on the console); preopens
  // follow from fd 3, where Rust's std discovers them.
  const wasi = new WASI(
    [],
    ["HOME=/home", "XDG_CONFIG_HOME=/home/.config"],
    [
      new OpenFile(new File([])),
      ConsoleStdout.lineBuffered((l) => console.log(`[pure-ffi] ${l}`)),
      ConsoleStdout.lineBuffered((l) => console.error(`[pure-ffi] ${l}`)),
      new PreopenDirectory("/home", homeRoot),
    ],
    { debug: false },
  );

  let measure: (text: string) => number = (t) => t.length * 8;

  const source = await WebAssembly.compileStreaming(
    fetch(`${import.meta.env.BASE_URL}pure_ffi.wasm?v=${__BUILD_ID__}`),
  );
  const instance = await WebAssembly.instantiate(source, {
    wasi_snapshot_preview1: wasi.wasiImport,
    purestudy: {
      pure_js_measure: (_ctx: number, ptr: number) => measure(cstrAt(ptr)),
    },
  });
  const exports = instance.exports as WasmEngine["exports"];
  wasi.initialize(instance as Parameters<WASI["initialize"]>[0]);

  // Views are created per access: memory.grow() detaches earlier buffers.
  const bytes = () => new Uint8Array(exports.memory.buffer);

  function cstrAt(ptr: number): string {
    const b = bytes();
    let end = ptr;
    while (b[end] !== 0) end++;
    return dec.decode(b.subarray(ptr, end));
  }

  const allocLens = new Map<number, number>();
  function inStr(s: string): number {
    const utf8 = enc.encode(s);
    const ptr = (exports.pure_web_alloc as (n: number) => number)(utf8.length + 1);
    bytes().set(utf8, ptr);
    bytes()[ptr + utf8.length] = 0;
    allocLens.set(ptr, utf8.length + 1);
    return ptr;
  }
  function freeStr(ptr: number): void {
    const len = allocLens.get(ptr);
    if (len !== undefined) {
      allocLens.delete(ptr);
      (exports.pure_web_free as (p: number, n: number) => void)(ptr, len);
    }
  }
  function takeStr(ptr: number): string | null {
    if (!ptr) return null;
    const s = cstrAt(ptr);
    (exports.pure_study_string_free as (p: number) => void)(ptr);
    return s;
  }
  function withErrSlot<T>(f: (slot: number) => T): [T, string | null] {
    const slot = (exports.pure_web_alloc as (n: number) => number)(4);
    new DataView(exports.memory.buffer).setUint32(slot, 0, true);
    const result = f(slot);
    const errPtr = new DataView(exports.memory.buffer).getUint32(slot, true);
    (exports.pure_web_free as (p: number, n: number) => void)(slot, 4);
    return [result, errPtr ? takeStr(errPtr) : null];
  }

  return {
    exports,
    inStr,
    freeStr,
    takeStr,
    withErrSlot,
    setMeasure(m) {
      measure = m;
    },
    measureFnptr: (exports.pure_web_measure_fnptr as () => number)(),
  };
}
