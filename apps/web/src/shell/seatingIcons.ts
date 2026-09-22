// The named seatings' icons (Material, 24×24 paths) and the catalogue key that
// names each, shared by whatever marks a seating: the History sheet today, the
// bookmarks row while it carried them (PlanChip.svelte). One table, because an
// icon that means "Sunday morning" in one place has to mean it everywhere.
//
// `other` — the everyday seating — is deliberately absent: it is the absence of
// a mark, and a glyph for "nowhere in particular" would sit on every second line.

export const SEATING_ICONS: Record<string, string> = {
  // wb_sunny — Sunday morning
  "sunday-morning":
    "M6.76 4.84l-1.8-1.79-1.41 1.41 1.79 1.79 1.42-1.41zM4 10.5H1v2h3v-2zm9-9.95h-2V3.5h2V.55zm7.45 3.91l-1.41-1.41-1.79 1.79 1.41 1.41 1.79-1.79zm-3.21 13.7l1.79 1.8 1.41-1.41-1.8-1.79-1.4 1.4zM20 10.5v2h3v-2h-3zm-8-5c-3.31 0-6 2.69-6 6s2.69 6 6 6 6-2.69 6-6-2.69-6-6-6zm-1 16.95h2V19.5h-2v2.95zm-7.45-3.91l1.41 1.41 1.79-1.8-1.41-1.41-1.79 1.8z",
  // nightlight_round — Sunday evening
  "sunday-evening":
    "M12 3a9 9 0 1 0 9 9c0-.46-.04-.92-.1-1.36a5.389 5.389 0 0 1-4.4 2.26 5.403 5.403 0 0 1-3.14-9.8c-.44-.06-.9-.1-1.36-.1z",
  // group — the midweek meeting
  "wednesday-evening":
    "M16 11c1.66 0 2.99-1.34 2.99-3S17.66 5 16 5c-1.66 0-3 1.34-3 3s1.34 3 3 3zm-8 0c1.66 0 2.99-1.34 2.99-3S9.66 5 8 5C6.34 5 5 6.34 5 8s1.34 3 3 3zm0 2c-2.33 0-7 1.17-7 3.5V19h14v-2.5c0-2.33-4.67-3.5-7-3.5zm8 0c-.29 0-.62.02-.97.05 1.16.84 1.97 1.97 1.97 3.45V19h6v-2.5c0-2.33-4.67-3.5-7-3.5z",
};

/** The catalogue key naming a seating — an icon's accessible name. */
export const SEATING_LABEL_KEY: Record<string, string> = {
  "sunday-morning": "bookmarks.sundayMorning",
  "sunday-evening": "bookmarks.sundayEvening",
  "wednesday-evening": "bookmarks.wednesdayEvening",
};
