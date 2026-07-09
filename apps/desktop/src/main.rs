//! pure-study desktop shell (GTK4 + libadwaita).
//!
//! **Current state:** a headless smoke test that proves the `core → layout`
//! pipeline against a hydrated data tree, before the GTK window is wired up
//! (next step — see PROGRESS.md). Run with the overlay data on hand:
//!
//! ```sh
//! OVERLAY_HOME=../overlay cargo run -p pure-desktop
//! ```

use pure_core::{corpus, VRef};
use pure_layout::{layout_chapter, LayoutConfig, Measure};

/// Stand-in measurement until the GTK/Pango-backed one lands: every character
/// is a fixed width. Good enough to exercise the layout algorithm headlessly.
struct MonoMeasure {
    char_w: f32,
}
impl Measure for MonoMeasure {
    fn text_width(&self, text: &str) -> f32 {
        text.chars().count() as f32 * self.char_w
    }
}

fn main() {
    let home = std::env::var("OVERLAY_HOME").unwrap_or_else(|_| ".".to_string());
    let kjv_path = format!("{home}/data/kjv.jsonl");

    let corpus = match corpus::load_corpus(&kjv_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("could not load corpus at {kjv_path}: {e}");
            eprintln!("Set OVERLAY_HOME to a hydrated overlay tree, e.g.:");
            eprintln!("  OVERLAY_HOME=../overlay cargo run -p pure-desktop");
            std::process::exit(1);
        }
    };

    println!(
        "loaded {} verses (tokenization: {})",
        corpus.len(),
        corpus.tokenization_version()
    );

    // Lay out John 3 and report.
    let verses = corpus.chapter_verses("John", 3);
    let measure = MonoMeasure { char_w: 9.0 };
    let cfg = LayoutConfig::default();
    let dl = layout_chapter(verses, &measure, &cfg);
    println!(
        "John 3 → {} placed boxes, {:.0}px tall at {:.0}px wide",
        dl.items.len(),
        dl.height,
        dl.width
    );

    if let Some(v) = corpus.verse(&VRef::new("John", 3, 16)) {
        println!("\nJohn 3:16\n  {}", v.body());
        // Show the per-word Strong's tags that a Ctrl+click would resolve.
        for (i, t) in v.tokens.iter().enumerate() {
            if !t.strongs.is_empty() {
                println!("  [{i:2}] {:<12} {}", t.word, t.strongs.join(", "));
            }
        }
    }
}
