/** The shipped-language code a locale tag resolves to, before the engine can
 *  answer. Mirrors `Lang::shipped` in `crates/core/src/i18n.rs` — the two must
 *  route alike or the splash, the stage-1 corpus pick and the engine disagree
 *  about what language a device is.
 *
 *  For every language but Chinese this is the base tag (`de-CH` → `de`). Chinese
 *  subtags choose between two shipped rows rather than narrowing one (a browser
 *  says `zh-TW` or `zh-Hans-CN`, never `zht`): script subtag first, then the
 *  traditional-script regions, then the mainland default, which is also what a
 *  bare `zh` usually means. Pure and rune-free, so the engine worker's pack logic
 *  can import it. */
export function shippedBase(tag: string | null | undefined): string {
  const parts = (tag ?? "").toLowerCase().split(/[-_]/);
  const base = parts[0] ?? "";
  if (base !== "zh") return base;
  const rest = parts.slice(1);
  if (rest.includes("hant")) return "zht";
  if (rest.includes("hans")) return "zhs";
  return rest.some((r) => r === "tw" || r === "hk" || r === "mo") ? "zht" : "zhs";
}
