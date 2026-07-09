//! pure-study desktop shell — GTK4 + libadwaita (the first native UI).
//!
//! A thin shell over `pure-core` + `pure-layout`: it measures scripture text
//! with cairo, hands the measurements to the shared layout engine, paints the
//! returned display list, and forwards clicks back to the layout's hit-test.
//! No study logic lives here — the same core will back the WinUI and Compose
//! shells.
//!
//! Measuring and painting with the *same* engine (cairo) guarantees the
//! per-word hit regions line up exactly with the glyphs on screen. (A later
//! refinement: swap cairo's toy font API for Pango + the bundled EB Garamond,
//! and gate Strong's lookup behind Ctrl+click as overlay does.)
//!
//! ```sh
//! OVERLAY_HOME=../overlay cargo run -p pure-desktop
//! ```

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::{cairo, glib};

use pure_core::corpus::{Corpus, FLAG_ADDED, FLAG_DIVINE, FLAG_TITLE};
use pure_core::strongs::{self, StrongsDict};
use pure_core::{canon, corpus};
use pure_layout::{layout_chapter, DisplayList, Hit, ItemKind, LayoutConfig, Measure};

const APP_ID: &str = "ca.cavallo.purestudy";
const MAX_COLUMN: f32 = 720.0;
const MARGIN: f32 = 28.0;

/// The whole reader state, shared across signal handlers.
struct State {
    corpus: Corpus,
    strongs: StrongsDict,
    book: String,
    chapter: u16,
    font_size: f64,
    /// The display list from the last paint, kept so clicks can hit-test
    /// against exactly what is on screen.
    dl: Option<DisplayList>,
    margin_x: f32,
    last_content_height: i32,
}

type Shared = Rc<RefCell<State>>;

/// cairo-backed text measurement: measure with the very context we paint into.
struct CairoMeasure<'a> {
    cr: &'a cairo::Context,
}
impl Measure for CairoMeasure<'_> {
    fn text_width(&self, text: &str) -> f32 {
        self.cr.text_extents(text).map(|e| e.x_advance() as f32).unwrap_or(0.0)
    }
}

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

fn load_state() -> Result<State, String> {
    let home = std::env::var("OVERLAY_HOME").unwrap_or_else(|_| ".".to_string());
    let corpus = corpus::load_corpus(format!("{home}/data/kjv.jsonl")).map_err(|e| e.to_string())?;
    let strongs =
        strongs::load_strongs(format!("{home}/data/strongs.json")).map_err(|e| e.to_string())?;
    Ok(State {
        corpus,
        strongs,
        book: "John".to_string(),
        chapter: 3,
        font_size: 21.0,
        dl: None,
        margin_x: 0.0,
        last_content_height: 0,
    })
}

fn build_ui(app: &adw::Application) {
    let state = match load_state() {
        Ok(s) => Rc::new(RefCell::new(s)),
        Err(e) => {
            present_error(app, &e);
            return;
        }
    };

    // ── header nav ───────────────────────────────────────────────────────────
    let header = adw::HeaderBar::new();
    let title = adw::WindowTitle::new("pure-study", "1769 KJV");
    header.set_title_widget(Some(&title));

    let prev_btn = gtk::Button::from_icon_name("go-previous-symbolic");
    prev_btn.set_tooltip_text(Some("Previous chapter"));
    let next_btn = gtk::Button::from_icon_name("go-next-symbolic");
    next_btn.set_tooltip_text(Some("Next chapter"));

    let book_names: Vec<&str> = canon::BOOKS.iter().map(|b| b.name).collect();
    let book_dd = gtk::DropDown::from_strings(&book_names);
    book_dd.set_tooltip_text(Some("Book"));

    let chapter_spin = gtk::SpinButton::with_range(1.0, 150.0, 1.0);
    chapter_spin.set_tooltip_text(Some("Chapter"));

    header.pack_start(&prev_btn);
    header.pack_start(&book_dd);
    header.pack_start(&chapter_spin);
    header.pack_start(&next_btn);

    // ── scripture canvas ───────────────────────────────────────────────────────
    let area = gtk::DrawingArea::new();
    area.set_hexpand(true);
    area.set_vexpand(true);
    area.add_css_class("scripture");

    {
        let state = state.clone();
        area.set_draw_func(move |area, cr, width, _height| draw_scripture(&state, area, cr, width));
    }

    let scripture_scroll = gtk::ScrolledWindow::new();
    scripture_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scripture_scroll.set_child(Some(&area));
    scripture_scroll.set_hexpand(true);
    scripture_scroll.set_vexpand(true);

    // ── study side panel ─────────────────────────────────────────────────────
    let study_label = gtk::Label::new(Some("Click a word to look up its Strong’s entry."));
    study_label.set_wrap(true);
    study_label.set_xalign(0.0);
    study_label.set_yalign(0.0);
    study_label.set_selectable(true);
    study_label.set_margin_top(16);
    study_label.set_margin_bottom(16);
    study_label.set_margin_start(16);
    study_label.set_margin_end(16);

    let study_scroll = gtk::ScrolledWindow::new();
    study_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    study_scroll.set_child(Some(&study_label));
    study_scroll.set_size_request(300, -1);

    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    paned.set_start_child(Some(&scripture_scroll));
    paned.set_end_child(Some(&study_scroll));
    paned.set_resize_start_child(true);
    paned.set_resize_end_child(false);
    paned.set_shrink_end_child(false);
    paned.set_position(700);

    // ── click → hit-test → Strong's ────────────────────────────────────────────
    {
        let state = state.clone();
        let study_label = study_label.clone();
        let click = gtk::GestureClick::new();
        click.connect_pressed(move |_g, _n, x, y| {
            let st = state.borrow();
            if let Some(dl) = &st.dl {
                let hx = x as f32 - st.margin_x;
                if let Some(hit) = dl.hit_test(hx, y as f32) {
                    study_label.set_markup(&study_markup(&st, &hit));
                }
            }
        });
        area.add_controller(click);
    }

    // ── wire nav ──────────────────────────────────────────────────────────────
    // A guard flag so programmatic widget updates don't re-enter the handlers.
    let guard = Rc::new(Cell::new(false));

    {
        let state = state.clone();
        let guard = guard.clone();
        let book_dd2 = book_dd.clone();
        let chapter_spin2 = chapter_spin.clone();
        let title2 = title.clone();
        let area2 = area.clone();
        book_dd.connect_selected_notify(move |dd| {
            if guard.get() {
                return;
            }
            let idx = dd.selected() as usize;
            if let Some(b) = canon::BOOKS.get(idx) {
                {
                    let mut st = state.borrow_mut();
                    st.book = b.id.to_string();
                    st.chapter = 1;
                }
                sync_widgets(&state, &guard, &book_dd2, &chapter_spin2, &title2, &area2);
            }
        });
    }

    {
        let state = state.clone();
        let guard = guard.clone();
        let book_dd2 = book_dd.clone();
        let chapter_spin2 = chapter_spin.clone();
        let title2 = title.clone();
        let area2 = area.clone();
        chapter_spin.connect_value_changed(move |spin| {
            if guard.get() {
                return;
            }
            state.borrow_mut().chapter = (spin.value() as u16).max(1);
            sync_widgets(&state, &guard, &book_dd2, &chapter_spin2, &title2, &area2);
        });
    }

    {
        let state = state.clone();
        let guard = guard.clone();
        let book_dd2 = book_dd.clone();
        let chapter_spin2 = chapter_spin.clone();
        let title2 = title.clone();
        let area2 = area.clone();
        prev_btn.connect_clicked(move |_| {
            {
                let mut st = state.borrow_mut();
                if st.chapter > 1 {
                    st.chapter -= 1;
                }
            }
            sync_widgets(&state, &guard, &book_dd2, &chapter_spin2, &title2, &area2);
        });
    }

    {
        let state = state.clone();
        let guard = guard.clone();
        let book_dd2 = book_dd.clone();
        let chapter_spin2 = chapter_spin.clone();
        let title2 = title.clone();
        let area2 = area.clone();
        next_btn.connect_clicked(move |_| {
            {
                let mut st = state.borrow_mut();
                let count = st.corpus.chapter_count(&st.book);
                if st.chapter < count {
                    st.chapter += 1;
                }
            }
            sync_widgets(&state, &guard, &book_dd2, &chapter_spin2, &title2, &area2);
        });
    }

    // ── assemble window ─────────────────────────────────────────────────────────
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&paned));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .default_width(1000)
        .default_height(760)
        .content(&toolbar)
        .build();

    install_css();

    // Initial widget sync (selects John 3) and first paint.
    sync_widgets(&state, &guard, &book_dd, &chapter_spin, &title, &area);
    window.present();
}

/// Push the current `State` into the nav widgets + title without re-entering
/// their handlers, then request a repaint.
fn sync_widgets(
    state: &Shared,
    guard: &Rc<Cell<bool>>,
    book_dd: &gtk::DropDown,
    chapter_spin: &gtk::SpinButton,
    title: &adw::WindowTitle,
    area: &gtk::DrawingArea,
) {
    let (book_idx, chapter, count, subtitle) = {
        let st = state.borrow();
        (
            canon::book_order(&st.book).unwrap_or(0) as u32,
            st.chapter as f64,
            st.corpus.chapter_count(&st.book).max(1) as f64,
            format!("{} {} · 1769 KJV", canon::display_name(&st.book), st.chapter),
        )
    };
    guard.set(true);
    book_dd.set_selected(book_idx);
    chapter_spin.set_range(1.0, count);
    chapter_spin.set_value(chapter);
    guard.set(false);
    title.set_subtitle(&subtitle);
    area.queue_draw();
}

/// Lay out and paint the current chapter. Measurement and painting share the
/// one cairo context, so the stored hit regions match the glyphs exactly.
fn draw_scripture(state: &Shared, area: &gtk::DrawingArea, cr: &cairo::Context, width: i32) {
    // warm paper background
    cr.set_source_rgb(0.988, 0.976, 0.957);
    let _ = cr.paint();

    let mut st = state.borrow_mut();
    cr.select_font_face("Serif", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    cr.set_font_size(st.font_size);

    let fe = cr.font_extents().unwrap_or_else(|_| panic!("cairo font_extents failed"));
    let ascent = fe.ascent();
    let line_height = (fe.height() * 1.4) as f32;
    let space_width = cr.text_extents(" ").map(|e| e.x_advance() as f32).unwrap_or(4.0);

    let col = ((width as f32) - 2.0 * MARGIN).min(MAX_COLUMN).max(60.0);
    let margin_x = ((width as f32) - col).max(2.0 * MARGIN) / 2.0;
    st.margin_x = margin_x;

    let cfg = LayoutConfig {
        width: col,
        line_height,
        space_width,
        verse_num_gap: space_width * 1.4,
        para_indent: line_height * 0.9,
        para_spacing: line_height * 0.45,
    };

    let verses = st.corpus.chapter_verses(&st.book, st.chapter).to_vec();
    let measure = CairoMeasure { cr };
    let dl = layout_chapter(&verses, &measure, &cfg);

    let top = MARGIN;
    for item in &dl.items {
        let px = (margin_x + item.x) as f64;
        let py = top as f64 + item.y as f64 + ascent as f64;
        match &item.kind {
            ItemKind::VerseNumber(_) => {
                cr.select_font_face("Serif", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
                cr.set_source_rgb(0.62, 0.49, 0.22); // gold
                cr.move_to(px, py);
                let _ = cr.show_text(&item.text);
                cr.select_font_face("Serif", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
            }
            ItemKind::Word { .. } => {
                let slant = if item.flags & FLAG_ADDED != 0 {
                    cairo::FontSlant::Italic // KJV supplied words: italic, dimmer
                } else {
                    cairo::FontSlant::Normal
                };
                cr.select_font_face("Serif", slant, cairo::FontWeight::Normal);
                if item.flags & FLAG_ADDED != 0 {
                    cr.set_source_rgb(0.42, 0.40, 0.38);
                } else if item.flags & FLAG_DIVINE != 0 {
                    cr.set_source_rgb(0.30, 0.20, 0.15); // divine name: darker
                } else if item.flags & FLAG_TITLE != 0 {
                    cr.set_source_rgb(0.40, 0.36, 0.30); // superscription
                } else {
                    cr.set_source_rgb(0.13, 0.12, 0.10);
                }
                cr.move_to(px, py);
                let _ = cr.show_text(&item.text);

                // words carrying a Strong's tag get a faint gold underline
                if !item.strongs.is_empty() {
                    cr.set_source_rgba(0.62, 0.49, 0.22, 0.30);
                    cr.set_line_width(1.0);
                    cr.move_to(px, py + 2.5);
                    cr.line_to(px + item.w as f64, py + 2.5);
                    let _ = cr.stroke();
                }
            }
        }
    }

    let content_height = (top + dl.height + MARGIN) as i32;
    st.dl = Some(dl);
    if st.last_content_height != content_height {
        st.last_content_height = content_height;
        area.set_content_height(content_height);
    }
}

/// Pango-markup describing a clicked word and its Strong's entries.
fn study_markup(st: &State, hit: &Hit) -> String {
    let esc = |s: &str| glib::markup_escape_text(s).to_string();

    let word = st
        .corpus
        .verse(&hit.verse)
        .and_then(|v| v.tokens.get(hit.token_index as usize))
        .map(|t| t.word.clone())
        .unwrap_or_default();

    let mut s = String::new();
    s.push_str(&format!(
        "<b>{}</b>\n<span size=\"xx-large\">{}</span>\n\n",
        esc(&hit.verse.display()),
        esc(&word)
    ));

    if hit.strongs.is_empty() {
        s.push_str("<i>no Strong’s tag on this word</i>");
        return s;
    }

    for code in &hit.strongs {
        s.push_str(&format!("<b>{}</b>\n", esc(code)));
        match st.strongs.get(code) {
            Some(e) => {
                if let Some(l) = &e.lemma {
                    s.push_str(&format!("<span size=\"x-large\">{}</span>  ", esc(l)));
                }
                if let Some(x) = &e.xlit {
                    s.push_str(&format!("<i>{}</i>", esc(x)));
                }
                if let Some(p) = &e.pron {
                    s.push_str(&format!("  <span foreground=\"#888\">/{}/</span>", esc(p)));
                }
                s.push('\n');
                if let Some(d) = &e.def {
                    s.push_str(&format!("\n{}\n", esc(d)));
                }
                if let Some(k) = &e.kjv {
                    s.push_str(&format!("\n<small>KJV: {}</small>\n", esc(k)));
                }
            }
            None => s.push_str("<i>(not in the dictionary)</i>\n"),
        }
        s.push('\n');
    }
    s
}

fn install_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        ".scripture { background: #fcf9f4; } \
         label { font-family: Serif; }",
    );
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn present_error(app: &adw::Application, msg: &str) {
    let label = gtk::Label::new(Some(&format!(
        "Could not load scripture data.\n\n{msg}\n\nSet OVERLAY_HOME to a hydrated overlay tree, e.g.\n  OVERLAY_HOME=../overlay cargo run -p pure-desktop"
    )));
    label.set_wrap(true);
    label.set_margin_top(40);
    label.set_margin_bottom(40);
    label.set_margin_start(40);
    label.set_margin_end(40);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&label));
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .default_width(560)
        .default_height(320)
        .content(&toolbar)
        .build();
    window.present();
}
