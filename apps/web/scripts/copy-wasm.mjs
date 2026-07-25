#!/usr/bin/env node
// Copy the wasm32-wasip1 build of pure-ffi into public/. Run after:
//   cargo build -p pure-ffi --release --target wasm32-wasip1
import { copyFileSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, "../../..");
const src = join(repo, "target/wasm32-wasip1/release/pure_ffi.wasm");
const dst = join(here, "../public/pure_ffi.wasm");
copyFileSync(src, dst);
console.log(`copied pure_ffi.wasm (${(statSync(dst).size / 1048576).toFixed(1)}MB)`);
