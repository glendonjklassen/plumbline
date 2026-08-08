// The reading map's paint, shared by every surface that shows it.
//
// The Android twin is `readingTint` in apps/android/.../ui/BookNav.kt — same
// alphas, same floors, same reasoning, so a chapter looks the same on a phone
// and in a browser. The core (crates/core/src/reading.rs) decides WHAT a chapter
// is; this only decides how loudly to say it.
//
// Hue says where you stand, strength says how much it wants your attention. A
// chapter read last week is barely tinted; one you have never opened is lit gold
// from the first launch. Those are two different invitations and they are meant to
// look different — "you have been away a while" and "there is treasure in here you
// have not seen".

/** What the core sends per chapter/book (the flattened `Heat`). */
export interface ReadingHeat {
  pct: number;
  standing: "unread" | "partial" | "read";
  glow: number;
  days?: number;
  lastRead?: string;
}

export interface ReadingTint {
  /** Tile background. */
  fill: string;
  /** Tile border. */
  border: string;
  /** The bloom, ready for `box-shadow`; empty string when there is nothing to say. */
  shadow: string;
}

/** Read a themed colour off the document, with a light-theme fallback so this
 *  works before the palette lands. */
function themed(name: string, fallback: string): string {
  const v =
    typeof getComputedStyle === "function"
      ? getComputedStyle(document.documentElement).getPropertyValue(`--${name}`).trim()
      : "";
  return v || fallback;
}

/** `#rrggbb` → `r, g, b` for use inside `rgba()`. Passes anything else through
 *  unchanged so a themed value that is already a function still works. */
function rgbParts(hex: string): string {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return hex;
  const n = parseInt(m[1], 16);
  return `${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}`;
}

const BASE: Record<string, [string, string]> = {
  read: ["readDone", "#6f8f6a"],
  partial: ["readPartial", "#a8642c"],
  unread: ["readUnread", "#c9a227"],
};

/** Resolve a chapter's or book's standing into CSS. Returns null when there is
 *  no reading data yet, so a caller can fall back to its plain styling. */
export function readingTint(heat: ReadingHeat | null | undefined): ReadingTint | null {
  if (!heat) return null;
  const [role, fallback] = BASE[heat.standing] ?? BASE.unread;
  const rgb = rgbParts(themed(role, fallback));
  const pct = Math.min(1, Math.max(0, heat.pct));
  const glow = Math.min(1, Math.max(0, heat.glow));
  // A floor so the hue is legible before any glow; a partway chapter deepens
  // with its own progress, so you can see movement without a number.
  const presence = heat.standing === "partial" ? 0.16 + 0.24 * pct : 0.1;
  const strength = Math.min(0.72, presence + glow * 0.42);
  return {
    fill: `rgba(${rgb}, ${(strength * 0.42).toFixed(3)})`,
    border: `rgba(${rgb}, ${Math.min(1, 0.28 + strength * 0.72).toFixed(3)})`,
    shadow:
      glow <= 0.02
        ? ""
        : `0 0 ${(4 + glow * 12).toFixed(1)}px ${(glow * 3).toFixed(1)}px rgba(${rgb}, ${(glow * 0.45).toFixed(3)})`,
  };
}

/** The tint as an inline `style` string for a grid tile. */
export function tintStyle(heat: ReadingHeat | null | undefined): string {
  const t = readingTint(heat);
  if (!t) return "";
  return `background:${t.fill};border-color:${t.border};${t.shadow ? `box-shadow:${t.shadow};` : ""}`;
}

/** A human sentence for the tile's tooltip — the map should be able to explain
 *  itself, since colour alone never can. */
export function tintTitle(name: string, heat: ReadingHeat | null | undefined): string {
  if (!heat) return name;
  if (heat.standing === "read") {
    const d = heat.days ?? 0;
    const when =
      d <= 0 ? "today" : d === 1 ? "yesterday" : d < 31 ? `${d} days ago` : d < 365 ? `${Math.round(d / 30)} months ago` : `${(d / 365).toFixed(1)} years ago`;
    return `${name} — read through, last ${when}`;
  }
  if (heat.standing === "partial") return `${name} — ${Math.round(heat.pct * 100)}% read`;
  return `${name} — not read yet`;
}
