import { test, expect } from "@playwright/test";
import { createHash } from "node:crypto";
import { gzipSync } from "node:zlib";
import { verifierFor } from "../src/engine/pack";

// The cold path's guard against storing a bad download. `verifierFor` runs inside
// `depotBytes` BEFORE the bytes are stored, so a truncated body or a captive
// portal's page served 200 is refused instead of being read back on every later
// boot. Tested as the pure function it is: the downloads it guards happen inside
// the engine worker, where page-level routing cannot reach (network.spec.ts).
//
// Fails against a verifier that never throws (both bad downloads would pass), and
// against one that ignored the gzip layer (the good file's hash is over the RAW
// bytes, so hashing the .gz would refuse it).

const raw = new TextEncoder().encode(JSON.stringify({ format: "x", body: "y".repeat(4096) }));
const sha16 = (b: Uint8Array) => createHash("sha256").update(b).digest("hex").slice(0, 16);
const file = { path: "data/example.json", hash: sha16(raw), gzBytes: 0, stage: "study" } as any;

test("a pack download is refused unless it matches its manifest hash", async () => {
  const verify = verifierFor(file)!;
  expect(verify, "a hashed file must get a verifier").toBeTruthy();

  let kept: Uint8Array | undefined;
  await verifierFor(file, (r) => (kept = r))!(new Uint8Array(gzipSync(raw)));
  expect(kept && Buffer.from(kept).equals(Buffer.from(raw)), "the good file's bytes come back unzipped").toBe(true);

  const truncated = new Uint8Array(gzipSync(raw)).subarray(0, 40);
  await expect(verify(truncated), "a truncated download was accepted").rejects.toThrow();

  const portal = new TextEncoder().encode("<html><body>Sign in to the Wi-Fi</body></html>");
  await expect(verify(portal), "an HTML page served 200 was accepted").rejects.toThrow(/did not match its hash/);

  expect(verifierFor({ ...file, hash: undefined }), "a file with no hash has nothing to check").toBeUndefined();
});
