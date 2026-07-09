//! pure-study desktop shell — GTK4 + libadwaita (the first native UI).
//!
//! A thin shell over `pure-core` + `pure-layout`: it measures scripture text
//! with cairo, hands the measurements to the shared layout engine, paints the
//! returned display list, and forwards clicks back to the layout's hit-test.
//! No study logic lives here — the same core will back the WinUI and Compose
//! shells.
//!
//! Beyond reading it now offers the core's study surface: a **search** box
//! (word / phrase / reference, multi-tier), a **concordance** (every verse a
//! Strong's number tags), the 1769 **margin notes**, plus **zoom** and
//! **keyboard** navigation. Search results and concordance entries are clickable
//! reference links that navigate — and scroll — the reader.
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
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::{cairo, gdk, glib};

use pure_core::corpus::{Corpus, FLAG_ADDED, FLAG_DIVINE, FLAG_TITLE};
use pure_core::search::{self, Notes, SearchAnswer, SearchIx};
use pure_core::strongs::{self, OccurrenceIx, StrongsDict};
use pure_core::weave::{self, LoadedWeave};
use pure_core::{canon, corpus, notes, VRef};
use pure_layout::{layout_chapter, DisplayList, Hit, ItemKind, LayoutConfig, Measure};

const APP_ID: &str = "ca.cavallo.purestudy";
const MAX_COLUMN: f32 = 720.0;
const MARGIN: f32 = 28.0;
const MIN_FONT: f64 = 12.0;
const MAX_FONT: f64 = 48.0;
/// How many concordance rows to list before trusting the reader to search.
const OCC_SHOWN: usize = 300;
/// How many cross-references to list for one verse before capping.
const XREF_SHOWN: usize = 40;

/// The whole reader state, shared across signal handlers.
struct State {
    corpus: Corpus,
    strongs: StrongsDict,
    search_ix: SearchIx,
    occ_ix: OccurrenceIx,
    notes: Notes,
    /// Verse → its weave cross-reference partners (deduped), precomputed once.
    xrefs: HashMap<VRef, Vec<Xref>>,
    book: String,
    chapter: u16,
    font_size: f64,
    /// Font family to render the scripture in ("EB Garamond" or a fallback).
    family: String,
    /// The display list from the last paint, kept so clicks can hit-test
    /// against exactly what is on screen.
    dl: Option<DisplayList>,
    margin_x: f32,
    last_content_height: i32,
    /// A verse to scroll to (and briefly tint) after the next paint — set when
    /// navigating from a search hit or concordance entry.
    scroll_to: Option<u16>,
    highlight: Option<u16>,
}

type Shared = Rc<RefCell<State>>;

/// One cross-reference: a verse the current verse is weave-linked to, plus the
/// weave that asserts it.
struct Xref {
    partner: VRef,
    weave: String,
}

/// Precompute, for every verse, its weave partners across all loaded weaves —
/// both directions of each undirected link, deduped by partner. This backs both
/// the reader's gutter marker and the study panel's cross-reference list.
fn build_xrefs(weaves: &[LoadedWeave]) -> HashMap<VRef, Vec<Xref>> {
    let mut map: HashMap<VRef, Vec<Xref>> = HashMap::new();
    for lw in weaves {
        for l in &lw.weave.links {
            map.entry(l.a.clone())
                .or_default()
                .push(Xref { partner: l.b.clone(), weave: lw.weave.name.clone() });
            map.entry(l.b.clone())
                .or_default()
                .push(Xref { partner: l.a.clone(), weave: lw.weave.name.clone() });
        }
    }
    for xs in map.values_mut() {
        let mut seen = HashSet::new();
        xs.retain(|x| seen.insert(x.partner.clone()));
    }
    map
}

/// Register the bundled EB Garamond with fontconfig for this process only (no
/// change to the user's installed fonts). Returns the family to render in — the
/// bundled face if it registered, else a plain serif fallback. Pango's default
/// cairo font map reads the current fontconfig config, so the family becomes
/// resolvable app-wide once added.
fn register_bundled_fonts() -> String {
    let base = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fonts/");
    let mut ok = true;
    for file in ["EBGaramond.ttf", "EBGaramond-Italic.ttf"] {
        match std::ffi::CString::new(format!("{base}{file}")) {
            Ok(path) => {
                // SAFETY: a plain fontconfig C call; a null config targets the
                // current default config (the one Pango's font map consults).
                let added = unsafe {
                    fontconfig_sys::FcConfigAppFontAddFile(
                        std::ptr::null_mut(),
                        path.as_ptr() as *const u8,
                    )
                };
                ok &= added != 0;
            }
            Err(_) => ok = false,
        }
    }
    if ok { "EB Garamond".to_string() } else { "Serif".to_string() }
}

/// The chrome widgets handlers need to read/update, bundled so closures capture
/// one cheap clone instead of a dozen. (GTK objects are refcounted handles.)
#[derive(Clone)]
struct Ui {
    book_dd: gtk::DropDown,
    chapter_spin: gtk::SpinButton,
    title: adw::WindowTitle,
    area: gtk::DrawingArea,
    study: gtk::Label,
    /// The study panel's scroller — shown only when there's something to study
    /// (a double-clicked word, search results, a concordance), hidden otherwise.
    study_scroll: gtk::ScrolledWindow,
    vadj: gtk::Adjustment,
    /// Guards programmatic widget updates from re-entering their handlers.
    guard: Rc<Cell<bool>>,
}

/// Show the study panel with `markup`; open it if it was hidden.
fn show_study(ui: &Ui, markup: &str) {
    ui.study.set_markup(markup);
    ui.study_scroll.set_visible(true);
}

/// Collapse the study panel (reader takes the full width).
fn hide_study(ui: &Ui) {
    ui.study_scroll.set_visible(false);
}

/// Pango-backed text measurement: the width comes from the very `pango::Layout`
/// (with the body font set) that then paints the runs, so hit regions and glyphs
/// stay in lock-step. Measuring the regular face — words later painted italic /
/// bold differ negligibly, exactly as the old cairo path did.
struct PangoMeasure<'a> {
    layout: &'a pango::Layout,
}
impl Measure for PangoMeasure<'_> {
    fn text_width(&self, text: &str) -> f32 {
        self.layout.set_text(text);
        // Fractional logical width (Pango units → px): avoids the per-token
        // rounding drift that `pixel_size` would accumulate across a line, so
        // hit boxes stay aligned with the painted glyph advances.
        self.layout.size().0 as f32 / pango::SCALE as f32
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
    // Notes are optional: a missing file is not an error.
    let notes = notes::load_notes(format!("{home}/data/kjv-notes.jsonl")).map_err(|e| e.to_string())?;
    // Weaves (cross-references) load from `home/weaves` (+ suggested); bad files
    // are reported but don't fail the reader.
    let (weaves, _weave_errs) = weave::load_weaves(&home);
    let xrefs = build_xrefs(&weaves);
    let search_ix = SearchIx::build(&corpus);
    let occ_ix = OccurrenceIx::build(&corpus);
    let family = register_bundled_fonts();
    Ok(State {
        corpus,
        strongs,
        search_ix,
        occ_ix,
        notes,
        xrefs,
        book: "John".to_string(),
        chapter: 3,
        font_size: 21.0,
        family,
        dl: None,
        margin_x: 0.0,
        last_content_height: 0,
        scroll_to: None,
        highlight: None,
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

    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("search — word, phrase, or reference"));
    search.set_width_chars(28);

    header.pack_start(&prev_btn);
    header.pack_start(&book_dd);
    header.pack_start(&chapter_spin);
    header.pack_start(&next_btn);
    header.pack_end(&search);

    // ── scripture canvas ───────────────────────────────────────────────────────
    let area = gtk::DrawingArea::new();
    area.set_hexpand(true);
    area.set_vexpand(true);
    area.set_focusable(true);
    area.add_css_class("scripture");

    let scripture_scroll = gtk::ScrolledWindow::new();
    scripture_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scripture_scroll.set_child(Some(&area));
    scripture_scroll.set_hexpand(true);
    scripture_scroll.set_vexpand(true);

    // ── study side panel ─────────────────────────────────────────────────────
    let study = gtk::Label::new(Some(
        "Double-click a word for its Strong’s entry, or search above.",
    ));
    study.set_wrap(true);
    study.set_xalign(0.0);
    study.set_yalign(0.0);
    study.set_selectable(true);
    study.set_use_markup(true);
    study.set_margin_top(16);
    study.set_margin_bottom(16);
    study.set_margin_start(16);
    study.set_margin_end(16);

    let study_scroll = gtk::ScrolledWindow::new();
    study_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    study_scroll.set_child(Some(&study));
    study_scroll.set_size_request(320, -1);

    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    paned.set_start_child(Some(&scripture_scroll));
    paned.set_end_child(Some(&study_scroll));
    paned.set_resize_start_child(true);
    paned.set_resize_end_child(false);
    paned.set_shrink_end_child(false);
    paned.set_position(700);

    // The study panel starts collapsed; it opens on demand.
    study_scroll.set_visible(false);

    let ui = Ui {
        book_dd: book_dd.clone(),
        chapter_spin: chapter_spin.clone(),
        title: title.clone(),
        area: area.clone(),
        study: study.clone(),
        study_scroll: study_scroll.clone(),
        vadj: scripture_scroll.vadjustment(),
        guard: Rc::new(Cell::new(false)),
    };

    // ── paint ──────────────────────────────────────────────────────────────────
    {
        let state = state.clone();
        area.set_draw_func(move |area, cr, width, _h| draw_scripture(&state, area, cr, width));
    }

    // ── double-click a word → its Strong's entry (+ verse notes + concordance) ──
    {
        let state = state.clone();
        let ui = ui.clone();
        let click = gtk::GestureClick::new();
        click.set_button(gdk::BUTTON_PRIMARY);
        click.connect_pressed(move |_g, n_press, x, y| {
            ui.area.grab_focus();
            // Strong's lookup is on the second click; a single click is left free
            // (future: selection). The reader paints everything offset down by
            // MARGIN, so undo that on the y before hit-testing the display list.
            if n_press != 2 {
                return;
            }
            let hit = {
                let st = state.borrow();
                st.dl
                    .as_ref()
                    .and_then(|dl| dl.hit_test(x as f32 - st.margin_x, y as f32 - MARGIN))
            };
            if let Some(hit) = hit {
                let markup = word_study_markup(&state.borrow(), &hit);
                show_study(&ui, &markup);
            }
        });
        area.add_controller(click);
    }

    // ── study-panel links (search hits / concordance / go-to) navigate ──────────
    {
        let state = state.clone();
        let ui = ui.clone();
        study.connect_activate_link(move |_label, uri| {
            handle_link(&state, &ui, uri);
            glib::Propagation::Stop
        });
    }

    // ── search box → results in the study panel ─────────────────────────────────
    {
        let state = state.clone();
        let ui = ui.clone();
        search.connect_search_changed(move |entry| {
            let q = entry.text().to_string();
            if q.trim().is_empty() {
                hide_study(&ui);
            } else {
                let markup = search_markup(&state.borrow(), &q);
                show_study(&ui, &markup);
            }
        });
    }

    // ── zoom: Ctrl + scroll ─────────────────────────────────────────────────────
    {
        let state = state.clone();
        let ui = ui.clone();
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        scroll.connect_scroll(move |ctrl, _dx, dy| {
            if ctrl.current_event_state().contains(gdk::ModifierType::CONTROL_MASK) {
                zoom(&state, &ui, if dy < 0.0 { 1.0 } else { -1.0 });
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        area.add_controller(scroll);
    }

    // ── nav handlers ────────────────────────────────────────────────────────────
    {
        let state = state.clone();
        let ui = ui.clone();
        book_dd.connect_selected_notify(move |dd| {
            if ui.guard.get() {
                return;
            }
            if let Some(b) = canon::BOOKS.get(dd.selected() as usize) {
                navigate(&state, &ui, b.id, 1, None);
            }
        });
    }
    {
        let state = state.clone();
        let ui = ui.clone();
        chapter_spin.connect_value_changed(move |spin| {
            if ui.guard.get() {
                return;
            }
            let (book, ch) = { let st = state.borrow(); (st.book.clone(), (spin.value() as u16).max(1)) };
            navigate(&state, &ui, &book, ch, None);
        });
    }
    {
        let state = state.clone();
        let ui = ui.clone();
        prev_btn.connect_clicked(move |_| step_chapter(&state, &ui, -1));
    }
    {
        let state = state.clone();
        let ui = ui.clone();
        next_btn.connect_clicked(move |_| step_chapter(&state, &ui, 1));
    }

    // ── keyboard: page scroll + chapter step ────────────────────────────────────
    {
        let state = state.clone();
        let ui = ui.clone();
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(move |_c, key, _code, mods| {
            let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
            match key {
                gdk::Key::Page_Down | gdk::Key::space => { page(&ui, 0.9); glib::Propagation::Stop }
                gdk::Key::Page_Up => { page(&ui, -0.9); glib::Propagation::Stop }
                gdk::Key::Home => { ui.vadj.set_value(ui.vadj.lower()); glib::Propagation::Stop }
                gdk::Key::End => {
                    ui.vadj.set_value(ui.vadj.upper() - ui.vadj.page_size());
                    glib::Propagation::Stop
                }
                gdk::Key::Right | gdk::Key::bracketright => { step_chapter(&state, &ui, 1); glib::Propagation::Stop }
                gdk::Key::Left | gdk::Key::bracketleft => { step_chapter(&state, &ui, -1); glib::Propagation::Stop }
                gdk::Key::plus | gdk::Key::equal if ctrl => { zoom(&state, &ui, 1.0); glib::Propagation::Stop }
                gdk::Key::minus if ctrl => { zoom(&state, &ui, -1.0); glib::Propagation::Stop }
                gdk::Key::Escape => { hide_study(&ui); glib::Propagation::Stop }
                _ => glib::Propagation::Proceed,
            }
        });
        area.add_controller(keys);
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
    sync_widgets(&state, &ui); // selects John 3 + first paint
    window.present();
    area.grab_focus();
}

/// Push the current `State` into the nav widgets + title without re-entering
/// their handlers, then request a repaint.
fn sync_widgets(state: &Shared, ui: &Ui) {
    let (book_idx, chapter, count, subtitle) = {
        let st = state.borrow();
        (
            canon::book_order(&st.book).unwrap_or(0) as u32,
            st.chapter as f64,
            st.corpus.chapter_count(&st.book).max(1) as f64,
            format!("{} {} · 1769 KJV", canon::display_name(&st.book), st.chapter),
        )
    };
    ui.guard.set(true);
    ui.book_dd.set_selected(book_idx);
    ui.chapter_spin.set_range(1.0, count);
    ui.chapter_spin.set_value(chapter);
    ui.guard.set(false);
    ui.title.set_subtitle(&subtitle);
    ui.area.queue_draw();
}

/// Go to `book`/`chapter`, optionally scrolling to (and tinting) `verse`.
fn navigate(state: &Shared, ui: &Ui, book: &str, chapter: u16, verse: Option<u16>) {
    {
        let mut st = state.borrow_mut();
        st.book = book.to_string();
        st.chapter = chapter.max(1);
        st.scroll_to = verse;
        st.highlight = verse;
    }
    sync_widgets(state, ui);
    // Reset to the top for a plain chapter change; a verse target scrolls after
    // the next paint has produced a display list to locate it in.
    if verse.is_none() {
        ui.vadj.set_value(ui.vadj.lower());
    } else {
        let state = state.clone();
        let ui = ui.clone();
        glib::timeout_add_local_once(Duration::from_millis(50), move || {
            scroll_to_pending(&state, &ui);
        });
    }
}

/// Step the chapter within the current book, clamped to its range.
fn step_chapter(state: &Shared, ui: &Ui, delta: i32) {
    let (book, ch) = {
        let st = state.borrow();
        let count = st.corpus.chapter_count(&st.book);
        let next = (st.chapter as i32 + delta).clamp(1, count.max(1) as i32) as u16;
        (st.book.clone(), next)
    };
    navigate(state, ui, &book, ch, None);
}

/// Scroll the reader so the pending target verse sits near the top, using the
/// last painted display list to find its position.
fn scroll_to_pending(state: &Shared, ui: &Ui) {
    let y = {
        let mut st = state.borrow_mut();
        let target = match st.scroll_to.take() {
            Some(v) => v,
            None => return,
        };
        st.dl.as_ref().and_then(|dl| {
            dl.items
                .iter()
                .find(|it| matches!(it.kind, ItemKind::VerseNumber(n) if n == target))
                .map(|it| MARGIN + it.y)
        })
    };
    if let Some(y) = y {
        let v = (y as f64 - 8.0).max(ui.vadj.lower());
        ui.vadj.set_value(v.min(ui.vadj.upper() - ui.vadj.page_size()));
    }
}

fn zoom(state: &Shared, ui: &Ui, dir: f64) {
    {
        let mut st = state.borrow_mut();
        st.font_size = (st.font_size + dir).clamp(MIN_FONT, MAX_FONT);
    }
    ui.area.queue_draw();
}

fn page(ui: &Ui, frac: f64) {
    let step = ui.vadj.page_size() * frac;
    let v = (ui.vadj.value() + step).clamp(ui.vadj.lower(), ui.vadj.upper() - ui.vadj.page_size());
    ui.vadj.set_value(v);
}

/// Parse a study-panel link and act on it: `go:Book:ch[:verse]` navigates,
/// `occ:CODE` opens that Strong's concordance.
fn handle_link(state: &Shared, ui: &Ui, uri: &str) {
    if let Some(rest) = uri.strip_prefix("go:") {
        let parts: Vec<&str> = rest.split(':').collect();
        if let [book, ch] = parts[..] {
            if let Ok(c) = ch.parse::<u16>() {
                navigate(state, ui, book, c, None);
            }
        } else if let [book, ch, v] = parts[..] {
            if let (Ok(c), Ok(v)) = (ch.parse::<u16>(), v.parse::<u16>()) {
                navigate(state, ui, book, c, Some(v));
            }
        }
    } else if let Some(code) = uri.strip_prefix("occ:") {
        let markup = concordance_markup(&state.borrow(), code);
        show_study(ui, &markup);
    }
}

/// Lay out and paint the current chapter. Measurement and painting share the
/// one cairo context, so the stored hit regions match the glyphs exactly.
fn draw_scripture(state: &Shared, area: &gtk::DrawingArea, cr: &cairo::Context, width: i32) {
    // warm paper background
    cr.set_source_rgb(0.988, 0.976, 0.957);
    let _ = cr.paint();

    let mut st = state.borrow_mut();

    // One Pango layout on this cairo context, reused to measure then paint, so
    // hit regions match glyphs exactly. Three font variants share family + size.
    let layout = pangocairo::functions::create_layout(cr);
    let mut regular = pango::FontDescription::new();
    regular.set_family(&st.family);
    regular.set_absolute_size(st.font_size * pango::SCALE as f64);
    let mut italic = regular.clone();
    italic.set_style(pango::Style::Italic);
    let mut bold = regular.clone();
    bold.set_weight(pango::Weight::Bold);
    layout.set_font_description(Some(&regular));

    let metrics = layout.context().metrics(Some(&regular), None);
    let ascent = metrics.ascent() as f32 / pango::SCALE as f32;
    let descent = metrics.descent() as f32 / pango::SCALE as f32;
    let line_height = (ascent + descent) * 1.35;
    layout.set_text(" ");
    let space_width = layout.size().0 as f32 / pango::SCALE as f32;

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
    let measure = PangoMeasure { layout: &layout };
    let dl = layout_chapter(&verses, &measure, &cfg);
    let top = MARGIN;
    let highlight = st.highlight;

    // Verses in this chapter carrying weave cross-references get a gutter dot.
    let xref_here: HashSet<u16> = verses
        .iter()
        .map(|v| v.verse)
        .filter(|&n| st.xrefs.contains_key(&VRef::new(&st.book, st.chapter, n)))
        .collect();

    // A soft band behind the target verse (from a search hit / concordance jump).
    if let Some(hv) = highlight {
        let ys: Vec<f32> = dl
            .items
            .iter()
            .filter(|it| item_verse_num(it) == Some(hv))
            .map(|it| it.y)
            .collect();
        if let (Some(&y0), Some(&y1)) = (ys.iter().next(), ys.iter().last()) {
            cr.set_source_rgba(0.62, 0.49, 0.22, 0.12);
            let _ = cr.rectangle(
                margin_x as f64 - 6.0,
                top as f64 + y0 as f64,
                col as f64 + 12.0,
                (y1 - y0) as f64 + line_height as f64,
            );
            let _ = cr.fill();
        }
    }

    for item in &dl.items {
        let px = (margin_x + item.x) as f64;
        let py = top as f64 + item.y as f64; // Pango paints from the top-left
        let baseline = py + ascent as f64;
        match &item.kind {
            ItemKind::VerseNumber(n) => {
                layout.set_font_description(Some(&bold));
                cr.set_source_rgb(0.62, 0.49, 0.22); // gold
                cr.move_to(px, py);
                layout.set_text(&item.text);
                pangocairo::functions::show_layout(cr, &layout);
                // A gutter dot marks a verse with weave cross-references.
                if xref_here.contains(n) {
                    cr.set_source_rgba(0.62, 0.49, 0.22, 0.75);
                    cr.arc((margin_x as f64 - 9.0).max(3.0), baseline - 4.0, 2.3, 0.0, std::f64::consts::TAU);
                    let _ = cr.fill();
                }
            }
            ItemKind::Word { .. } => {
                if item.flags & FLAG_ADDED != 0 {
                    layout.set_font_description(Some(&italic));
                    cr.set_source_rgb(0.42, 0.40, 0.38);
                } else {
                    layout.set_font_description(Some(&regular));
                    if item.flags & FLAG_DIVINE != 0 {
                        cr.set_source_rgb(0.30, 0.20, 0.15);
                    } else if item.flags & FLAG_TITLE != 0 {
                        cr.set_source_rgb(0.40, 0.36, 0.30);
                    } else {
                        cr.set_source_rgb(0.13, 0.12, 0.10);
                    }
                }
                cr.move_to(px, py);
                layout.set_text(&item.text);
                pangocairo::functions::show_layout(cr, &layout);

                if !item.strongs.is_empty() {
                    cr.set_source_rgba(0.62, 0.49, 0.22, 0.30);
                    cr.set_line_width(1.0);
                    cr.move_to(px, baseline + 2.5);
                    cr.line_to(px + item.w as f64, baseline + 2.5);
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

/// The verse number a placed item belongs to (its own number for a marker; for
/// a word, parsed back from its `VRef`). Used to band the highlighted verse.
fn item_verse_num(it: &pure_layout::PlacedItem) -> Option<u16> {
    match &it.kind {
        ItemKind::VerseNumber(n) => Some(*n),
        ItemKind::Word { verse, .. } => Some(verse.verse),
    }
}

// ── study-panel markup ──────────────────────────────────────────────────────

fn esc(s: &str) -> String {
    glib::markup_escape_text(s).to_string()
}

/// Markup for a clicked word: its Strong's entries, this verse's margin notes,
/// and a link into each code's concordance.
fn word_study_markup(st: &State, hit: &Hit) -> String {
    let word = st
        .corpus
        .verse(&hit.verse)
        .and_then(|v| v.tokens.get(hit.token_index as usize))
        .map(|t| t.word.clone())
        .unwrap_or_default();

    let mut s = format!(
        "<b>{}</b>\n<span size=\"xx-large\">{}</span>\n\n",
        esc(&hit.verse.display()),
        esc(&word)
    );

    if hit.strongs.is_empty() {
        s.push_str("<i>no Strong’s tag on this word</i>\n");
    }
    for code in &hit.strongs {
        s.push_str(&format!("<b>{}</b>   ", esc(code)));
        let count = st.occ_ix.count(code);
        s.push_str(&format!(
            "<a href=\"occ:{}\"><span size=\"small\">{} occurrence{} ▸</span></a>\n",
            esc(code),
            count,
            if count == 1 { "" } else { "s" }
        ));
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

    // Weave cross-references touching this verse, if any.
    if let Some(xs) = st.xrefs.get(&hit.verse) {
        s.push_str(&format!("\n<b>cross-references ({})</b>\n", xs.len()));
        for x in xs.iter().take(XREF_SHOWN) {
            s.push_str(&format!(
                "{}  <small><span foreground=\"#888\">{}</span></small>\n",
                go_link(&x.partner),
                esc(&x.weave)
            ));
        }
        if xs.len() > XREF_SHOWN {
            s.push_str(&format!("<small>… {} more</small>\n", xs.len() - XREF_SHOWN));
        }
    }

    // The 1769 translators' margin notes on this verse, if any.
    if let Some(ns) = st.notes.get(&hit.verse) {
        s.push_str("\n<b>margin notes</b>\n");
        for n in ns {
            s.push_str(&format!("<small>{}</small>\n", esc(n)));
        }
    }
    s
}

fn go_link(v: &VRef) -> String {
    format!(
        "<a href=\"go:{}:{}:{}\">{}</a>",
        esc(&v.book),
        v.chapter,
        v.verse,
        esc(&v.display())
    )
}

/// Markup for a search query's answer: a go-to jump, or a list of hit links.
fn search_markup(st: &State, query: &str) -> String {
    match search::run_search(&st.corpus, &st.notes, &st.search_ix, query) {
        None => "<i>Type a word, phrase, or reference (e.g. “love”, “God so loved”, “John 3”).</i>"
            .to_string(),
        Some(SearchAnswer::GoTo { book, chapter, verse }) => {
            let disp = match verse {
                Some(v) => VRef::new(book.clone(), chapter, v).display(),
                None => format!("{} {}", canon::display_name(&book), chapter),
            };
            let href = match verse {
                Some(v) => format!("go:{book}:{chapter}:{v}"),
                None => format!("go:{book}:{chapter}"),
            };
            format!(
                "<b>go to</b>\n\n<a href=\"{}\"><span size=\"large\">{}</span></a>",
                esc(&href),
                esc(&disp)
            )
        }
        Some(SearchAnswer::Hits { how, total, hits }) => {
            let mut s = format!(
                "<b>{} result{}</b>  <small>{}</small>\n\n",
                total,
                if total == 1 { "" } else { "s" },
                esc(&how)
            );
            for h in &hits {
                let why = if h.why.is_empty() {
                    String::new()
                } else {
                    format!("  <small><span foreground=\"#888\">{}</span></small>", esc(&h.why))
                };
                let note = if h.note { "  <small>※ note</small>" } else { "" };
                s.push_str(&format!("{}{}{}\n", go_link(&h.vref), why, note));
            }
            if total > hits.len() {
                s.push_str(&format!("\n<small>… {} more</small>", total - hits.len()));
            }
            s
        }
    }
}

/// Markup for a Strong's code's concordance: every verse it tags, as jump links.
fn concordance_markup(st: &State, code: &str) -> String {
    let verses = st.occ_ix.verses(code);
    let lemma = st
        .strongs
        .get(code)
        .and_then(|e| e.lemma.clone())
        .map(|l| format!("  <span size=\"large\">{}</span>", esc(&l)))
        .unwrap_or_default();
    let mut s = format!(
        "<b>{}</b>{}\n<small>{} occurrence{}</small>\n\n",
        esc(code),
        lemma,
        verses.len(),
        if verses.len() == 1 { "" } else { "s" }
    );
    for v in verses.iter().take(OCC_SHOWN) {
        s.push_str(&format!("{}\n", go_link(v)));
    }
    if verses.len() > OCC_SHOWN {
        s.push_str(&format!("\n<small>… {} more</small>", verses.len() - OCC_SHOWN));
    }
    s
}

fn install_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        ".scripture { background: #fcf9f4; } \
         label { font-family: \"EB Garamond\", serif; }",
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
