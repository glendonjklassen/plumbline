/* A C consumer smoke test for the Plumbline C ABI.
 *
 * Proves the generated header + ABI are correct by driving the whole surface
 * from C against the real corpus: open -> TOC -> layout (with a C measure
 * callback) -> hit-test -> Strong's -> search -> free.
 *
 * Build + run (from the repo root, after `cargo build -p plumbline-ffi --release`):
 *   cc -I crates/ffi/include crates/ffi/bindings/c/smoke.c \
 *      -L target/release -lplumbline_ffi -lpthread -ldl -lm \
 *      -Wl,-rpath,'$ORIGIN' -o /tmp/plumbline_smoke
 *   /tmp/plumbline_smoke /path/to/home
 *
 * This is intentionally NOT a real text engine — it approximates advance width
 * by byte length so the ABI (not typography) is what is under test.
 */
#include "plumbline.h"
#include <stdio.h>
#include <string.h>

/* A shell would back this with Pango/DirectWrite/Android; here: ~9px/char. */
static float measure(void *ctx, const char *text) {
  (void)ctx;
  return text ? (float)strlen(text) * 9.0f : 0.0f;
}

static void print_head(const char *label, char *json) {
  if (!json) {
    printf("  %-14s <null>\n", label);
    return;
  }
  printf("  %-14s %.140s%s\n", label, json, strlen(json) > 140 ? " ..." : "");
  plumbline_string_free(json);
}

int main(int argc, char **argv) {
  const char *home = argc > 1 ? argv[1] : ".";

  char *ver = plumbline_version();
  printf("Plumbline core v%s\n", ver);
  plumbline_string_free(ver);

  char *err = NULL;
  PlumblineEngine *engine = plumbline_engine_open(home, &err);
  if (!engine) {
    fprintf(stderr, "open failed: %s\n", err ? err : "(no message)");
    plumbline_string_free(err);
    return 1;
  }
  printf("opened engine from %s\n", home);

  printf("John chapters: %u\n", plumbline_engine_chapter_count(engine, "John"));

  /* Lay out John 3 and hit-test the middle of the first word box. */
  struct PlumblineLayoutConfig cfg = {
      .width = 640.0f,       .line_height = 28.0f,  .space_width = 6.0f,
      .verse_num_gap = 8.0f, .para_indent = 24.0f,  .para_spacing = 12.0f,
  };
  PlumblineDisplayList *dl =
      plumbline_engine_layout_chapter(engine, "John", 3, cfg, measure, NULL);
  printf("laid out John 3: %u items, %.0fpx tall\n",
         plumbline_layout_item_count(dl), plumbline_layout_height(dl));

  /* Probe a spot on the first line for whatever word sits there. */
  print_head("hit(60,14):", plumbline_layout_hit_test_json(dl, 60.0f, 14.0f));

  print_head("verse:", plumbline_engine_verse_json(engine, "John 3:16"));
  print_head("strongs G2316:", plumbline_engine_strongs_json(engine, "G2316"));
  print_head("occurrences:", plumbline_engine_strongs_occurrences_json(engine, "G2316"));
  print_head("search 'love':", plumbline_engine_search_json(engine, "love"));
  print_head("search 'John 3':", plumbline_engine_search_json(engine, "John 3"));

  plumbline_layout_free(dl);
  plumbline_engine_free(engine);
  printf("OK: all handles/strings freed cleanly.\n");
  return 0;
}
