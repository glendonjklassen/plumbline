import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";

import { launchDestination } from "../src/shell/church";

// The install identity: the webmanifest, the icons it names, and the colour the
// browser paints its own chrome with.
//
// Everything here fails INVISIBLY on this machine. A manifest that names an icon
// size it does not ship still shows a perfect icon in every previewer, because
// the previewer reads the file and the launcher reads the declaration. A maskable
// icon whose mark reaches the edges looks flawless as a square and gets its arms
// shaved off by the circle Android crops it to. A single `theme-color` looks right
// in exactly the colour scheme the author happens to be using. So each test below
// DECODES the bytes the server returns, or reads the HTML the server returns, and
// checks them against what the declaration claims.
//
// Written 2026-07-29 for the pre-release audit item about icons and the manifest.
//
// -- WHY `id` IS "/" (the unforgiving one) ------------------------------------
// `id` is the PWA's permanent install identity. Changing it later does not
// migrate anything: the old install is orphaned and the app installs again beside
// it, with the reader's IndexedDB left behind the identity they no longer use.
//
// It is resolved against the manifest's ORIGIN, not the manifest's path, so there
// is no host-agnostic spelling of it the way `start_url: "./"` is host-agnostic.
// "/" on the one origin this app is shared from is byte-for-byte the identity the
// app ALREADY had, because a manifest with no `id` takes its identity from
// `start_url`, and `start_url: "./"` resolves to the origin root. So declaring it
// changes nothing today — which is the entire point of declaring it now rather
// than later. `start_url` is about to become interesting (the punch list has an
// item that mirrors the open chapter into the URL), and the moment `start_url`
// moves, an undeclared `id` moves with it and every existing install is orphaned.
// The first test below holds `id` and today's implicit identity equal.
//
// -- WHY THERE ARE NO `screenshots` -------------------------------------------
// A screenshot in the manifest is what a browser's richer install dialog shows.
// The repo has exactly one product image (`assets/readme/reader-weaves.png`,
// 1920×1004, a desktop-width reader), and a manifest screenshot wants
// `form_factor: "narrow"` shots taken on a real phone at a real device pixel
// ratio — which needs the maintainer and a device, not a resize of a desktop
// capture. Inventing entries would be worse than having none: a named file that
// 404s makes some browsers drop the whole richer-install treatment. The field is
// therefore absent, and the test below is written so that ADDING it is checked
// rather than blocked — every entry's file must decode at the size it declares.

/** Registered manifest categories (w3c/manifest wiki). Anything else is ignored
 *  by every browser, so an invented one is a silent no-op. */
const KNOWN_CATEGORIES = [
  "books",
  "business",
  "education",
  "entertainment",
  "finance",
  "fitness",
  "food",
  "games",
  "government",
  "health",
  "kids",
  "lifestyle",
  "magazines",
  "medical",
  "music",
  "navigation",
  "news",
  "personalization",
  "photo",
  "politics",
  "productivity",
  "security",
  "shopping",
  "social",
  "sports",
  "travel",
  "utilities",
  "weather",
];

/** The `paper` hex of a theme, read out of the core's palette.
 *
 *  Read rather than hardcoded on purpose: the browser chrome and the page it
 *  frames must be the SAME colour, and the light palette already moved once
 *  (2026-07-29, the WCAG pass). A copied hex would have survived that silently. */
function corePaper(theme: "Light" | "Dark" | "Night"): string {
  const src = readFileSync(new URL("../../../crates/core/src/theme.rs", import.meta.url), "utf8");
  const arm = src.split(`Theme::${theme} => Palette {`)[1];
  if (!arm) throw new Error(`theme.rs has no Theme::${theme} palette arm — this test needs updating`);
  const hex = /paper:\s*"(#[0-9a-f]{6})"/.exec(arm);
  if (!hex) throw new Error(`theme.rs Theme::${theme} has no paper hex — this test needs updating`);
  return hex[1];
}

const LIGHT_PAPER = corePaper("Light");
const DARK_PAPER = corePaper("Dark");

type Manifest = {
  id?: string;
  name?: string;
  short_name?: string;
  description?: string;
  lang?: string;
  dir?: string;
  categories?: string[];
  start_url?: string;
  scope?: string;
  display?: string;
  orientation?: string;
  background_color?: string;
  theme_color?: string;
  icons?: { src: string; sizes?: string; type?: string; purpose?: string }[];
  screenshots?: { src: string; sizes?: string; type?: string; form_factor?: string }[];
  shortcuts?: {
    name?: string;
    short_name?: string;
    url?: string;
    icons?: { src: string; sizes?: string; type?: string }[];
  }[];
};

/** Fetch the manifest the page actually links, and parse it AS JSON.
 *
 *  A webmanifest is parsed by a JSON parser, not a JS one: a trailing comma or a
 *  comment makes the whole file unusable and a browser's only complaint is a
 *  console line nobody reads. Returns the href too, because `id` and `start_url`
 *  are relative to different bases. */
async function manifest(
  page: import("@playwright/test").Page,
): Promise<{ href: string; text: string; json: Manifest }> {
  const link = page.locator('link[rel="manifest"]').first();
  await expect(link, "the page links no webmanifest — nothing can be installed").toHaveCount(1);
  const href = await page.evaluate(
    (h) => new URL(h, location.href).href,
    (await link.getAttribute("href")) ?? "",
  );
  const res = await page.request.get(href);
  expect(res.status(), `the manifest href points at ${href}, which is not being served`).toBe(200);
  const text = await res.text();
  let json: Manifest;
  try {
    json = JSON.parse(text) as Manifest;
  } catch (e) {
    throw new Error(`the webmanifest is not valid JSON, so every field in it is dead: ${String(e)}`);
  }
  return { href, text, json };
}

/** Decode an image the manifest names and measure it, in the page so the fetch
 *  goes through the same origin (and service worker) a real install would use.
 *
 *  `markRadius` is the distance from the centre to the farthest pixel that is not
 *  exactly the paper colour — i.e. how far the MARK reaches. That is the only
 *  honest way to check a maskable icon's safe zone: the declaration cannot say it
 *  and the square rendering cannot show it. */
async function decode(
  page: import("@playwright/test").Page,
  src: string,
  paper: string,
): Promise<{
  status: number;
  decoded: boolean;
  width: number;
  height: number;
  transparentCorner: boolean;
  cornersArePaper: boolean;
  markPixels: number;
  markRadius: number;
}> {
  return await page.evaluate(
    async ([u, paperHex]) => {
      const res = await fetch(new URL(u, location.href).href);
      const blank = {
        status: res.status,
        decoded: false,
        width: 0,
        height: 0,
        transparentCorner: false,
        cornersArePaper: false,
        markPixels: 0,
        markRadius: 0,
      };
      if (!res.ok) return blank;
      // A dev server (and some hosts) answer an unknown path with the SPA shell
      // at 200 rather than a 404, so "the file is missing" arrives here as HTML
      // that will not decode. Catch it and say that, instead of throwing
      // InvalidStateError out of the page.
      let bmp: ImageBitmap;
      try {
        bmp = await createImageBitmap(await res.blob());
      } catch {
        return blank;
      }
      const c = new OffscreenCanvas(bmp.width, bmp.height);
      const ctx = c.getContext("2d")!;
      ctx.drawImage(bmp, 0, 0);
      const { data } = ctx.getImageData(0, 0, bmp.width, bmp.height);
      const pr = parseInt(paperHex.slice(1, 3), 16);
      const pg = parseInt(paperHex.slice(3, 5), 16);
      const pb = parseInt(paperHex.slice(5, 7), 16);
      const cx = (bmp.width - 1) / 2;
      const cy = (bmp.height - 1) / 2;
      let markPixels = 0;
      let markRadius = 0;
      for (let y = 0; y < bmp.height; y++) {
        for (let x = 0; x < bmp.width; x++) {
          const i = (y * bmp.width + x) * 4;
          const opaquePaper =
            data[i + 3] === 255 && data[i] === pr && data[i + 1] === pg && data[i + 2] === pb;
          if (opaquePaper) continue;
          markPixels++;
          markRadius = Math.max(markRadius, Math.hypot(x - cx, y - cy));
        }
      }
      const corners: [number, number][] = [
        [0, 0],
        [bmp.width - 1, 0],
        [0, bmp.height - 1],
        [bmp.width - 1, bmp.height - 1],
      ];
      const at = (x: number, y: number) => (y * bmp.width + x) * 4;
      return {
        status: res.status,
        decoded: true,
        width: bmp.width,
        height: bmp.height,
        transparentCorner: corners.some(([x, y]) => data[at(x, y) + 3] < 255),
        cornersArePaper: corners.every(([x, y]) => {
          const i = at(x, y);
          return data[i + 3] === 255 && data[i] === pr && data[i + 1] === pg && data[i + 2] === pb;
        }),
        markPixels,
        markRadius,
      };
    },
    [src, paper] as const,
  );
}

test("the webmanifest parses and declares a complete, permanent identity", async ({ page }) => {
  await page.goto("/");
  const { href, json } = await manifest(page);

  // Every field an install dialog, a launcher and a task switcher read. A missing
  // one is not an error anywhere — the browser just guesses.
  for (const key of [
    "id",
    "name",
    "short_name",
    "description",
    "lang",
    "categories",
    "start_url",
    "scope",
    "display",
    "orientation",
    "background_color",
    "icons",
  ] as const) {
    expect(json[key], `the webmanifest has no \`${key}\``).toBeTruthy();
  }

  // THE ONE THAT CANNOT BE CHANGED LATER. `id` resolves against the ORIGIN;
  // an absent `id` means the identity IS `start_url`, resolved against the
  // manifest url. Holding those two equal is what makes this declaration a
  // freeze of the existing identity rather than a new one — see the header.
  const identity = await page.evaluate(
    ([id, startUrl, manifestHref]) => ({
      declared: new URL(id, location.origin).href,
      implied: new URL(startUrl, manifestHref).href,
    }),
    [json.id!, json.start_url!, href] as const,
  );
  expect(
    identity.declared,
    `\`id\` resolves to ${identity.declared} but this app's identity before \`id\` existed was ${identity.implied} (start_url). Changing it orphans every install that already exists`,
  ).toBe(identity.implied);

  // The rest: values a browser actually acts on.
  expect(json.lang, "manifest lang must match <html lang>").toBe(
    await page.locator("html").getAttribute("lang"),
  );
  expect(["ltr", "rtl", "auto"]).toContain(json.dir ?? "auto");
  expect(json.display, "standalone is what makes it feel installed").toBe("standalone");
  // The Android shell does not lock rotation (no android:screenOrientation, and
  // it handles the orientation config change itself), so the web shell must not
  // either — a reader with a keyboard case or a tablet reads in landscape.
  expect(json.orientation, "the shells do not lock rotation").toBe("any");
  expect(json.categories!.length).toBeGreaterThan(0);
  for (const c of json.categories!) {
    expect(KNOWN_CATEGORIES, `"${c}" is not a registered manifest category, so it does nothing`).toContain(
      c,
    );
  }
  expect(json.background_color, "the splash background is the reader's paper").toBe(LIGHT_PAPER);

  // NO `theme_color`, and its absence is a fix, not an omission (2026-08-28).
  //
  // The manifest is the ONE surface the running page can never write to. On
  // Android an installed PWA is a WebAPK, and the manifest's theme_color is
  // baked into it at install time; when a foldable's open/close re-creates the
  // activity, Chrome can fall back to that baked value and STOP consulting the
  // page's meta tags at all — a full reload with correct tags in the DOM still
  // shows a cream bar over a dark page, and it stays until the reader
  // uninstalls (which "fixes" it only because reinstalling re-mints the WebAPK).
  // Every in-page re-assert (session.svelte.ts, chrome-reassert.spec.ts) is
  // helpless against it, because the UA is no longer reading what they write.
  //
  // A static manifest cannot name a colour that is right in both polarities, so
  // the only correct fallback is NO fallback: with nothing baked in, the page's
  // media-scoped meta pair — which is right for every theme, and re-asserted at
  // every moment a UA can re-derive — is the only claim there is.
  expect(
    json.theme_color,
    "theme_color is back in the manifest — a foldable's activity re-creation falls back to this baked, " +
      "light-only value over the page's meta tags and sticks there until reinstall (Pixel Fold, 2026-08-28)",
  ).toBeUndefined();
});

test("every icon the manifest declares exists at exactly the size it claims", async ({ page }) => {
  await page.goto("/");
  const { json } = await manifest(page);
  const icons = json.icons!;

  // 192 and 512 are not a style choice: Chrome needs a >=144 icon to offer
  // installation at all and a 512 for the splash screen, and Android crops a
  // maskable one for the launcher.
  const purposes = icons.map((i) => i.purpose ?? "any");
  const anySizes = icons.filter((_, n) => purposes[n].split(/\s+/).includes("any")).map((i) => i.sizes);
  expect(anySizes, "the install icons must include 192x192 (launcher)").toContain("192x192");
  expect(anySizes, "the install icons must include 512x512 (splash screen)").toContain("512x512");
  expect(
    purposes.filter((p) => p.split(/\s+/).includes("maskable")).length,
    "exactly one maskable icon: none means Android shaves the mark, two is ambiguous",
  ).toBe(1);

  for (const icon of icons) {
    expect(icon.sizes, `${icon.src} declares no sizes, so a launcher has to guess`).toMatch(
      /^\d+x\d+$/,
    );
    expect(icon.type).toBe("image/png");
    const [w, h] = icon.sizes!.split("x").map(Number);
    const got = await decode(page, icon.src, LIGHT_PAPER);
    expect(got.status, `the manifest names ${icon.src}, which is not being served`).toBe(200);
    expect(
      got.decoded,
      `the manifest names ${icon.src}, and what the server returns for it is not a decodable image`,
    ).toBe(true);
    expect(
      { width: got.width, height: got.height },
      `${icon.src} is really ${got.width}×${got.height} but the manifest claims ${icon.sizes} — the launcher believes the manifest`,
    ).toEqual({ width: w, height: h });
    expect(
      got.markPixels,
      `${icon.src} is nearly empty — only ${got.markPixels} of its ${w * h} pixels differ from the paper colour, so whatever it draws is not the mark`,
    ).toBeGreaterThan(0.02 * w * h);
  }
});

test("the maskable icon survives being cropped to a circle", async ({ page }) => {
  await page.goto("/");
  const { json } = await manifest(page);
  const maskable = json.icons!.find((i) => (i.purpose ?? "").split(/\s+/).includes("maskable"))!;
  const [w] = maskable.sizes!.split("x").map(Number);
  const got = await decode(page, maskable.src, LIGHT_PAPER);

  // A launcher fills whatever it does not receive. Transparent corners become
  // whatever the launcher feels like — often black — behind a cream tile.
  expect(
    got.transparentCorner,
    `${maskable.src} has transparent corners; a launcher composites those on its own colour, not on paper`,
  ).toBe(false);
  expect(
    got.cornersArePaper,
    `${maskable.src} does not bleed the paper colour ${LIGHT_PAPER} to its corners — a maskable icon's background must fill the whole square`,
  ).toBe(true);

  // The safe zone: a circle of 80% of the width (radius 0.4w) is all a maskable
  // icon is guaranteed to keep. Measured, because nothing about the square
  // rendering shows it — the shaved arms only appear on a phone.
  const safe = 0.4 * w;
  expect(
    got.markRadius,
    `${maskable.src}'s mark reaches ${got.markRadius.toFixed(1)}px from the centre, past the ${safe}px safe radius (40% of ${w}) — Android's circle/squircle crop will cut it`,
  ).toBeLessThanOrEqual(safe);
  // And it is not tiny: an icon that passes the line above by being mostly empty
  // paper is a different bug.
  expect(
    got.markRadius,
    `${maskable.src}'s mark only reaches ${got.markRadius.toFixed(1)}px of ${w} — it will look lost inside the launcher's crop`,
  ).toBeGreaterThan(0.2 * w);
});

test("the browser chrome follows the reader's colour scheme", async ({ page }) => {
  // Read the HTML the SERVER sends, not the live DOM: `session.applyTheme()`
  // rewrites the first theme-color tag's content once the engine's palette
  // arrives, so the DOM cannot tell us what a browser sees while the page is
  // still loading — which is the whole window this pair exists to cover.
  const html = await (await page.request.get("/")).text();
  const tags = [...html.matchAll(/<meta\s+name="theme-color"[^>]*>/g)].map((m) => m[0]);
  expect(
    tags.length,
    "one theme-color is one colour: a dark-mode reader gets a cream address bar above a candlelit page",
  ).toBe(2);
  // Order is load-bearing: a UA takes the FIRST tag whose media matches, so an
  // unscoped tag anywhere in the pair would swallow the other one.
  expect(tags[0], "the first theme-color must be scoped to the light scheme").toContain(
    'media="(prefers-color-scheme: light)"',
  );
  expect(
    tags[0],
    `the light theme-color is not theme.rs's Theme::Light paper (${LIGHT_PAPER}) — the browser chrome would sit a shade off the page it frames`,
  ).toContain(`content="${LIGHT_PAPER}"`);
  expect(tags[1], "the second theme-color must be scoped to the dark scheme").toContain(
    'media="(prefers-color-scheme: dark)"',
  );
  expect(
    tags[1],
    `the dark theme-color is not theme.rs's Theme::Dark paper (${DARK_PAPER}) — that is what ThemeChoice::System resolves a dark device to`,
  ).toContain(`content="${DARK_PAPER}"`);

  // And they resolve the way the spec says: the first tag whose media matches
  // wins. Checked in a real browser under both schemes, AFTER the app has
  // applied its palette — which is also where the live value legitimately parts
  // company with the static pair above. That pair is the SPLASH's chrome and
  // the splash paints `paper`; once the palette lands, `Session.chrome` names
  // the surface actually under the bar, which is the header's `--paneNavBg`.
  // The two have the same polarity in all eighteen palettes, so the handoff can
  // move the tint by a shade and can never flip an icon.
  //
  // So the live half is stated against the app's OWN answer rather than a hex:
  // whatever the running session resolved, that is what the tag the UA reads
  // has to carry, and the polarity has to be the one the device asked for. A
  // rewrite that inverted either would fail here.
  for (const scheme of ["light", "dark"] as const) {
    await page.emulateMedia({ colorScheme: scheme });
    await page.goto("/");
    await expect
      .poll(
        () =>
          page.evaluate(() => {
            const s = (window as any).__plumbline;
            if (!s) return null; // the engine has not handed over yet
            let tag: string | null = null;
            for (const m of document.querySelectorAll<HTMLMetaElement>('meta[name="theme-color"]')) {
              const media = m.getAttribute("media");
              if (!media || matchMedia(media).matches) {
                tag = m.getAttribute("content");
                break;
              }
            }
            return {
              agrees: (tag ?? "").toLowerCase() === (s.chrome.color as string).toLowerCase(),
              dark: s.chrome.dark as boolean,
            };
          }),
        {
          timeout: 60_000,
          message: `the tag a ${scheme}-scheme UA reads has to be the colour this session resolved, at the polarity this device asked for`,
        },
      )
      .toEqual({ agrees: true, dark: scheme === "dark" });
  }
});

test("the install icons are part of the offline shell", async ({ page }) => {
  // `publicFiles` in vite.config.ts is BOTH the precache set and the depot's
  // keep-set, and it is maintained by hand — so an icon the manifest names and
  // that list does not is an icon the sweep deletes, and an install started on a
  // device with no network then has no icon. (social.spec.ts checks the same
  // thing for the icons index.html LINKS; these are the ones only the manifest
  // knows about.)
  await page.goto("/");
  const { json } = await manifest(page);
  const config = readFileSync(new URL("../vite.config.ts", import.meta.url), "utf8");
  for (const icon of json.icons!) {
    const name = icon.src.replace(/^\.?\//, "");
    expect(
      config,
      `the manifest names ${name} but vite.config.ts publicFiles does not — the depot sweep will delete it`,
    ).toContain(`"${name}"`);
  }
});

test("any screenshot the manifest names is real and the size it says", async ({ page }) => {
  // Vacuous today: there are no shipped screenshots (see the header for what
  // taking them needs). It is here so that adding the field is CHECKED — a
  // manifest screenshot that 404s costs the richer install dialog entirely.
  await page.goto("/");
  const { json } = await manifest(page);
  for (const shot of json.screenshots ?? []) {
    expect(shot.sizes, `${shot.src} declares no sizes`).toMatch(/^\d+x\d+$/);
    expect(
      shot.form_factor,
      `${shot.src} declares no form_factor, so it is offered for every device shape`,
    ).toBeTruthy();
    const [w, h] = shot.sizes!.split("x").map(Number);
    const got = await decode(page, shot.src, LIGHT_PAPER);
    expect(got.status, `the manifest names screenshot ${shot.src}, which is not being served`).toBe(
      200,
    );
    expect(
      got.decoded,
      `the manifest names screenshot ${shot.src}, and what the server returns for it is not a decodable image`,
    ).toBe(true);
    expect(
      { width: got.width, height: got.height },
      `screenshot ${shot.src} is ${got.width}×${got.height}, not the ${shot.sizes} it claims`,
    ).toEqual({ width: w, height: h });
  }
});

// -- SHORTCUTS -----------------------------------------------------------------
// A `shortcuts` entry is stored by the LAUNCHER at install time: the long-press
// menu keeps whatever URL the manifest declared that day, and nothing revalidates
// it against the app. So the check has to run the other way round — every URL the
// manifest hands the launcher must be one `launchDestination` (src/shell/church.ts,
// the app's own whitelist) accepts. Holding the two to each other is what makes a
// renamed destination fail HERE, instead of shipping a shortcut that boots to the
// reader as if it had never been tapped.
test("every launcher shortcut names a destination the app itself routes", async ({ page }) => {
  await page.goto("/");
  const { href, json } = await manifest(page);
  const shortcuts = json.shortcuts ?? [];
  expect(shortcuts.length, "the manifest declares no shortcuts — the long-press menu is empty").toBeGreaterThan(0);

  for (const sc of shortcuts) {
    expect(sc.name, "a shortcut with no name renders as a blank menu row").toBeTruthy();
    expect(sc.url, `shortcut "${sc.name}" has no url`).toBeTruthy();

    // In scope, or the launcher silently drops the entry.
    const u = new URL(sc.url!, href);
    expect(u.origin, `${sc.url} leaves the app's origin`).toBe(new URL(href).origin);

    expect(
      launchDestination(u.search),
      `${sc.url} names a destination launchDestination() rejects — tapping it would boot to the reader`,
    ).not.toBeNull();

    // Same contract as the install icons: the file must exist and be the size
    // the declaration claims, because the launcher believes the manifest.
    for (const icon of sc.icons ?? []) {
      expect(icon.sizes, `shortcut icon ${icon.src} declares no sizes`).toMatch(/^\d+x\d+$/);
      const [w, h] = icon.sizes!.split("x").map(Number);
      const got = await decode(page, icon.src, LIGHT_PAPER);
      expect(got.status, `shortcut icon ${icon.src} is not being served`).toBe(200);
      expect(got.decoded, `shortcut icon ${icon.src} is not a decodable image`).toBe(true);
      expect(
        { width: got.width, height: got.height },
        `shortcut icon ${icon.src} is really ${got.width}×${got.height}, not the ${icon.sizes} it claims`,
      ).toEqual({ width: w, height: h });
    }
  }
});
