<script lang="ts" module>
  // Re-exported for the components that render a share surface; it lives with
  // the sharing logic (shell/church.ts).
  export { PWA_URL } from "./church";
</script>

<script lang="ts">
  // A QR of whatever we're handing over. A shared link can carry the sender's
  // church (shell/church.ts), so the code is encoded at render time — one scan
  // gives someone both the Bible and the people who sent it.
  //
  // qrcode-generator (MIT, no dependencies of its own) does the encoding.
  // Verified locally by DECODING what it produces with zxing-cpp — 32 cases
  // covering both ECC levels, versions 1 through ~15, non-ASCII church names
  // and a full-length share link, all read back exactly.
  import qrcode from "qrcode-generator";
  import { PWA_URL } from "./church";
  import { t } from "../lib/i18n.svelte";

  // The library's default byte conversion is ASCII-only, and its ESM build
  // doesn't ship the UTF-8 one (`stringToBytesFuncs` exists only in the CJS
  // entry), so a church named "Iglesia Bíblica" would encode as mojibake.
  // TextEncoder is exactly what that missing function does.
  qrcode.stringToBytes = (s: string) => Array.from(new TextEncoder().encode(s));

  interface Props {
    /** Rendered edge in CSS px (the QR itself; the white quiet zone is inside). */
    size?: number;
    /** What the code encodes. */
    text?: string;
  }
  let { size = 148, text = PWA_URL }: Props = $props();

  const QUIET = 2; // quiet-zone modules per side (the page's white adds the rest)

  const modules = $derived.by(() => {
    const encode = (s: string) => {
      const q = qrcode(0, "M"); // 0 = smallest version that fits
      q.addData(s);
      q.make();
      const n = q.getModuleCount();
      return Array.from({ length: n }, (_, y) => Array.from({ length: n }, (_, x) => q.isDark(y, x)));
    };
    try {
      return encode(text);
    } catch {
      // A church typed to absurd length must not take the dialog down: fall
      // back to the bare app link, which always fits.
      return encode(PWA_URL);
    }
  });
  const n = $derived(modules.length);
</script>

<!-- Always dark-on-white regardless of theme: scanners want contrast. -->
<svg
  width={size}
  height={size}
  viewBox="0 0 {n + 2 * QUIET} {n + 2 * QUIET}"
  role="img"
  aria-label={t("qr.label")}
  shape-rendering="crispEdges"
>
  <rect width={n + 2 * QUIET} height={n + 2 * QUIET} fill="#ffffff" />
  {#each modules as row, y (y)}
    {#each row as dark, x (x)}
      {#if dark}
        <rect x={x + QUIET} y={y + QUIET} width="1" height="1" fill="#101010" />
      {/if}
    {/each}
  {/each}
</svg>
