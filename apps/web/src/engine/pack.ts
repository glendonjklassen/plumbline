// Fetch the read-only data pack (built by scripts/build-web-pack.mjs):
// manifest first, then the gzipped files, decompressed with the browser's
// DecompressionStream. HTTP-level caching is left to the service worker /
// Cache API layer; this module just loads bytes and reports progress.

export interface PackManifest {
  version: string;
  files: { path: string; bytes: number; gzBytes: number; stock?: boolean }[];
}

export interface PackProgress {
  /** 0..1 across the whole pack download, weighted by gzipped size. */
  fraction: number;
  currentFile: string;
}

const PACK_BASE = `${import.meta.env.BASE_URL}pack/`;

export async function fetchManifest(): Promise<PackManifest> {
  const res = await fetch(`${PACK_BASE}manifest.json`);
  if (!res.ok) throw new Error(`data pack manifest: HTTP ${res.status}`);
  return res.json();
}

async function gunzip(body: ArrayBuffer): Promise<Uint8Array> {
  const bytes = new Uint8Array(body);
  // Some servers see the .gz extension and serve it with Content-Encoding:
  // gzip — the browser then hands us already-decompressed bytes. Sniff the
  // gzip magic instead of trusting headers, so any host behaviour works.
  if (bytes.length < 2 || bytes[0] !== 0x1f || bytes[1] !== 0x8b) return bytes;
  const stream = new Blob([body]).stream().pipeThrough(new DecompressionStream("gzip"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/** Load every pack file, returning home-relative path → raw bytes. */
export async function fetchPack(
  manifest: PackManifest,
  onProgress?: (p: PackProgress) => void,
): Promise<Map<string, Uint8Array>> {
  const totalGz = manifest.files.reduce((s, f) => s + f.gzBytes, 0);
  let doneGz = 0;
  const out = new Map<string, Uint8Array>();
  // Fetch a few files concurrently; decompression overlaps the network.
  const queue = [...manifest.files];
  const workers = Array.from({ length: 4 }, async () => {
    for (let f = queue.shift(); f; f = queue.shift()) {
      onProgress?.({ fraction: doneGz / totalGz, currentFile: f.path });
      const res = await fetch(`${PACK_BASE}${f.path}.gz?v=${manifest.version}`);
      if (!res.ok) throw new Error(`data pack file ${f.path}: HTTP ${res.status}`);
      out.set(f.path, await gunzip(await res.arrayBuffer()));
      doneGz += f.gzBytes;
      onProgress?.({ fraction: doneGz / totalGz, currentFile: f.path });
    }
  });
  await Promise.all(workers);
  return out;
}
