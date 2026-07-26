<script lang="ts" module>
  // QR code of the hosted PWA (version 3, ECC M, 29×29), pre-generated so the
  // app stays offline and dependency-free. The matrix is a build-time constant;
  // regenerate after a URL change with:
  //   python3 -c "import qrcode; q=qrcode.QRCode(error_correction=qrcode.constants.ERROR_CORRECT_M, border=0); q.add_data('<url>'); q.make(fit=True); print('\n'.join(''.join('1' if c else '0' for c in r) for r in q.modules))"
  // (pip install qrcode). Keep this in sync with the Android twin (QrShare.kt).
  export const PWA_URL = "https://plumblinebible.org/";
  const MODULES = [
    "11111110101001101110101111111",
    "10000010001001110101001000001",
    "10111010010110100100101011101",
    "10111010110110000010001011101",
    "10111010110100000110001011101",
    "10000010100010001101101000001",
    "11111110101010101010101111111",
    "00000000110001011110100000000",
    "10001011110011000100111111001",
    "00000100010011101000001111111",
    "01111111000000001000011000001",
    "11000001100110100101010011011",
    "01110010010011111101110000010",
    "10101001011100000010001111111",
    "11100111010101110010100001101",
    "01000001110001011110011000011",
    "10000010011101000110110100010",
    "10101000111001101110001111011",
    "00101110111000001010010100101",
    "00001000011110100111001010011",
    "11010011100101111011111111001",
    "00000000100010000111100010001",
    "11111110111111110011101011101",
    "10000010010011011001100010000",
    "10111010101101000001111111001",
    "10111010011101101010110000010",
    "10111010000011101101010001111",
    "10000010000111100110100101011",
    "11111110100010011010101010010",
  ];
  const N = MODULES.length;
  const QUIET = 2; // quiet-zone modules on each side (spec wants ≥4 incl. page white)
</script>

<script lang="ts">
  interface Props {
    /** Rendered edge in CSS px (the QR itself; the white quiet zone is inside). */
    size?: number;
  }
  let { size = 148 }: Props = $props();
</script>

<!-- Always dark-on-white regardless of theme: scanners want contrast. -->
<svg
  width={size}
  height={size}
  viewBox="0 0 {N + 2 * QUIET} {N + 2 * QUIET}"
  role="img"
  aria-label="QR code linking to the Plumbline web app"
  shape-rendering="crispEdges"
>
  <rect width={N + 2 * QUIET} height={N + 2 * QUIET} fill="#ffffff" />
  {#each MODULES as row, y (y)}
    {#each row as cell, x (x)}
      {#if cell === "1"}
        <rect x={x + QUIET} y={y + QUIET} width="1" height="1" fill="#101010" />
      {/if}
    {/each}
  {/each}
</svg>
