//! pure-study desktop shell — GTK4 + libadwaita (the first native UI).
//!
//! A thin shell over `pure-core` + `pure-layout`: it shapes scripture text with
//! Pango, hands per-token widths to the shared layout engine, paints the
//! returned display list, and forwards clicks back to the layout's hit-test.
//! No study logic lives here — the same core will back the WinUI and Compose
//! shells.
//!
//! The reader is **multi-pane**: one or more columns, each showing an
//! independent book/chapter with its own nav and scroll, so passages can be read
//! side by side (overlay's core reading model). The active pane (the last one
//! touched) is what search results, cross-references, and the study panel act
//! on. Beyond reading it offers the core's study surface — multi-tier **search**,
//! a **concordance**, 1769 **margin notes**, and weave **cross-references** — set
//! in the bundled **EB Garamond**, with zoom and keyboard navigation.
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
use pure_core::reference::{CANON_SEGMENTS, OT_NT_DIVIDE};
use pure_core::tag::{self, LoadedTag, TagTarget};
use pure_core::thread::{self, LoadedThread};
use pure_core::weave::{self, Link, LoadedWeave, Span, WeaveKind};
use pure_core::{canon, corpus, notes, VRef};
use pure_layout::{layout_chapter, DisplayList, Hit, ItemKind, LayoutConfig, Measure};

const APP_ID: &str = "ca.cavallo.purestudy";
const MAX_COLUMN: f32 = 720.0;
const MARGIN: f32 = 28.0;
const MIN_FONT: f64 = 12.0;
const MAX_FONT: f64 = 48.0;
/// Most reading columns at once.
const MAX_PANES: usize = 3;
/// How many concordance rows to list before trusting the reader to search.
const OCC_SHOWN: usize = 300;
/// How many cross-references to list for one verse before capping.
const XREF_SHOWN: usize = 40;

/// Shared reader state: the immutable study core plus the live panes.
struct State {
    corpus: Corpus,
    strongs: StrongsDict,
    search_ix: SearchIx,
    occ_ix: OccurrenceIx,
    notes: Notes,
    /// Verse → its weave cross-reference partners (deduped), precomputed once.
    xrefs: HashMap<VRef, Vec<Xref>>,
    /// The loaded weaves, kept so link authoring can find-or-create by name.
    weaves: Vec<LoadedWeave>,
    /// Every weave link as a canonical, deduped verse pair — for the ambient
    /// connector lines drawn between panes.
    links: Vec<(VRef, VRef)>,
    /// Personal study data: threads (ordered passage trails) and tags.
    threads: Vec<LoadedThread>,
    tags: Vec<LoadedTag>,
    /// The data home, kept so authoring can write + reload study data.
    home: String,
    /// Font family to render the scripture in ("EB Garamond" or a fallback).
    family: String,
    font_size: f64,
    panes: Vec<Pane>,
    /// Which pane search / cross-references / the study panel act on.
    active: usize,
}

/// One reading column: what it shows plus its last paint (for hit-testing and
/// scroll-to-verse).
struct Pane {
    book: String,
    chapter: u16,
    dl: Option<DisplayList>,
    margin_x: f32,
    last_h: i32,
    scroll_to: Option<u16>,
    highlight: Option<u16>,
    /// A word (or word span) pinned as a link endpoint, for authoring weave
    /// links. `None` until the reader clicks a word in this pane.
    pin: Option<PinSpan>,
}

/// A pinned link endpoint: a token span within one verse of a pane. A single
/// click pins one word (`lo == hi == anchor`); clicking another word in the
/// same verse grows the span from the anchor to the newly-clicked token, so
/// the selection can be widened or narrowed without clearing it.
#[derive(Clone, Copy)]
struct PinSpan {
    verse: u16,
    anchor: u32,
    lo: u32,
    hi: u32,
}

impl Pane {
    fn new(book: &str, chapter: u16) -> Pane {
        Pane {
            book: book.to_string(),
            chapter,
            dl: None,
            margin_x: 0.0,
            last_h: 0,
            scroll_to: None,
            highlight: None,
            pin: None,
        }
    }
}

type Shared = Rc<RefCell<State>>;

/// One cross-reference: a verse the current verse is weave-linked to, plus the
/// weave that asserts it.
struct Xref {
    partner: VRef,
    weave: String,
}

/// Precompute, for every verse, its weave partners across all loaded weaves —
/// both directions of each undirected link, deduped by partner. Backs both the
/// reader's gutter marker and the study panel's cross-reference list.
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

/// All weave links as canonical, deduped verse pairs (core stores each link
/// with `a <= b` in reading order already), for the ambient connector lines.
fn build_links(weaves: &[LoadedWeave]) -> Vec<(VRef, VRef)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for lw in weaves {
        for l in &lw.weave.links {
            if seen.insert((l.a.clone(), l.b.clone())) {
                out.push((l.a.clone(), l.b.clone()));
            }
        }
    }
    out
}

/// Register the bundled EB Garamond with fontconfig for this process only (no
/// change to the user's installed fonts). Returns the family to render in — the
/// bundled face if it registered, else a plain serif fallback.
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

/// Persistent chrome plus the live per-pane widget handles. Cheap to clone
/// (GTK objects are refcounted; the pane list is shared via `Rc`).
#[derive(Clone)]
struct Ui {
    title: adw::WindowTitle,
    study: gtk::Label,
    /// The study panel's scroller — shown only when there's something to study.
    study_scroll: gtk::ScrolledWindow,
    /// Horizontal container the pane columns live in; rebuilt on add/close.
    pane_row: gtk::Box,
    /// Transparent layer over the panes where cross-reference connectors draw.
    link_layer: gtk::DrawingArea,
    /// The canon-overview strip under the panes (book map + per-pane pins).
    canon_map: gtk::DrawingArea,
    /// Header "＋ link" button — sensitive once ≥2 panes have a pinned endpoint.
    link_btn: gtk::Button,
    /// Per-pane widgets, kept parallel to `State::panes`.
    pane_uis: Rc<RefCell<Vec<PaneUi>>>,
    /// Guards programmatic widget updates from re-entering their handlers.
    guard: Rc<Cell<bool>>,
}

/// The GTK handles for one pane column.
#[derive(Clone)]
struct PaneUi {
    root: gtk::Box,
    area: gtk::DrawingArea,
    vadj: gtk::Adjustment,
    book_dd: gtk::DropDown,
    chapter_spin: gtk::SpinButton,
}

/// Show the study panel with `markup`; open it if it was hidden.
fn show_study(ui: &Ui, markup: &str) {
    ui.study.set_markup(markup);
    ui.study_scroll.set_visible(true);
}

/// Collapse the study panel (the panes take the full width).
fn hide_study(ui: &Ui) {
    ui.study_scroll.set_visible(false);
}

/// Pango-backed text measurement: the width comes from the very `pango::Layout`
/// (with the body font set) that then paints the runs, so hit regions and glyphs
/// stay in lock-step.
struct PangoMeasure<'a> {
    layout: &'a pango::Layout,
}
impl Measure for PangoMeasure<'_> {
    fn text_width(&self, text: &str) -> f32 {
        self.layout.set_text(text);
        // Fractional logical width (Pango units → px): avoids the per-token
        // rounding drift `pixel_size` would accumulate across a line.
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
    let links = build_links(&weaves);
    let (threads, _thread_errs) = thread::load_threads(&home);
    let (tags, _tag_errs) = tag::load_tags(&home);
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
        weaves,
        links,
        threads,
        tags,
        home,
        family,
        font_size: 21.0,
        panes: vec![Pane::new("John", 3)],
        active: 0,
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

    // ── header: brand/title + global search (acts on the active pane) ──────────
    let header = adw::HeaderBar::new();
    let title = adw::WindowTitle::new("pure-study", "1769 KJV");
    header.set_title_widget(Some(&title));

    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("search — word, phrase, or reference"));
    search.set_width_chars(28);
    header.pack_end(&search);

    let threads_btn = gtk::Button::with_label("Threads");
    threads_btn.add_css_class("flat");
    let tags_btn = gtk::Button::with_label("Tags");
    tags_btn.add_css_class("flat");
    let suggested_btn = gtk::Button::with_label("Suggested");
    suggested_btn.add_css_class("flat");
    suggested_btn.set_tooltip_text(Some("Review proposed weaves — approve to keep, reject to discard"));
    let link_btn = gtk::Button::with_label("＋ link");
    link_btn.add_css_class("flat");
    link_btn.set_tooltip_text(Some("Weave the two pinned words (click a word in each pane; click another word in the same verse to widen the span)"));
    link_btn.set_sensitive(false);
    header.pack_start(&threads_btn);
    header.pack_start(&tags_btn);
    header.pack_start(&suggested_btn);
    header.pack_start(&link_btn);

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
    study_scroll.set_visible(false); // opens on demand

    // ── panes container + connector overlay ─────────────────────────────────────
    let pane_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    pane_row.set_hexpand(true);
    pane_row.set_vexpand(true);
    pane_row.set_homogeneous(true);

    // A transparent layer over the panes for cross-reference connector lines; it
    // never intercepts input (clicks/scroll fall through to the panes).
    let link_layer = gtk::DrawingArea::new();
    link_layer.set_can_target(false);

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&pane_row));
    overlay.add_overlay(&link_layer);

    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    paned.set_start_child(Some(&overlay));
    paned.set_end_child(Some(&study_scroll));
    paned.set_resize_start_child(true);
    paned.set_resize_end_child(false);
    paned.set_shrink_end_child(false);
    paned.set_position(700);
    paned.set_vexpand(true);

    // ── canon-overview strip (book map, under the panes) ────────────────────────
    let canon_map = gtk::DrawingArea::new();
    canon_map.set_content_height(30);
    canon_map.set_hexpand(true);
    canon_map.set_tooltip_text(Some("Jump anywhere — click a book"));

    let ui = Ui {
        title,
        study: study.clone(),
        study_scroll,
        pane_row,
        link_layer: link_layer.clone(),
        canon_map: canon_map.clone(),
        link_btn: link_btn.clone(),
        pane_uis: Rc::new(RefCell::new(Vec::new())),
        guard: Rc::new(Cell::new(false)),
    };

    {
        let state = state.clone();
        let ui = ui.clone();
        link_btn.connect_clicked(move |_| make_link(&state, &ui));
    }

    // ── connector lines: draw weave links whose endpoints are both on screen ────
    {
        let state = state.clone();
        let ui = ui.clone();
        link_layer.set_draw_func(move |layer, cr, w, h| draw_links(&state, &ui, layer, cr, w, h));
    }

    // ── canon strip: draw + click-to-jump the active pane ───────────────────────
    {
        let state = state.clone();
        canon_map.set_draw_func(move |_a, cr, w, h| draw_canon(&state, cr, w, h));
    }
    {
        let state = state.clone();
        let ui = ui.clone();
        let click = gtk::GestureClick::new();
        click.connect_pressed(move |_g, _n, x, _y| {
            let w = ui.canon_map.width();
            if w <= 0 {
                return;
            }
            let frac = (x / w as f64).clamp(0.0, 0.999);
            let idx = (frac * canon::BOOKS.len() as f64) as usize;
            if let Some(b) = canon::BOOKS.get(idx) {
                let active = state.borrow().active;
                navigate_pane(&state, &ui, active, b.id, 1, None);
            }
        });
        canon_map.add_controller(click);
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

    // ── Threads / Tags browse buttons ───────────────────────────────────────────
    {
        let state = state.clone();
        let ui = ui.clone();
        threads_btn.connect_clicked(move |_| {
            let m = threads_list_markup(&state.borrow());
            show_study(&ui, &m);
        });
    }
    {
        let state = state.clone();
        let ui = ui.clone();
        tags_btn.connect_clicked(move |_| {
            let m = tags_list_markup(&state.borrow());
            show_study(&ui, &m);
        });
    }
    {
        let state = state.clone();
        let ui = ui.clone();
        suggested_btn.connect_clicked(move |_| {
            let m = suggested_list_markup(&state.borrow());
            show_study(&ui, &m);
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

    // ── assemble window ─────────────────────────────────────────────────────────
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&paned);
    content.append(&canon_map);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&content));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .default_width(1100)
        .default_height(780)
        .content(&toolbar)
        .build();

    install_css();
    rebuild_panes(&state, &ui); // builds the pane columns + first paint
    window.present();
}

/// Tear down and recreate every pane column from `State::panes`. Called on
/// startup and whenever a pane is added or closed, so handlers always capture a
/// valid, current index.
fn rebuild_panes(state: &Shared, ui: &Ui) {
    while let Some(child) = ui.pane_row.first_child() {
        ui.pane_row.remove(&child);
    }
    ui.pane_uis.borrow_mut().clear();

    let n = state.borrow().panes.len();
    for i in 0..n {
        let (root, pane_ui) = build_pane(state, ui, i, n);
        ui.pane_row.append(&root);
        ui.pane_uis.borrow_mut().push(pane_ui);
    }
    for i in 0..n {
        sync_pane(state, ui, i);
    }
    update_active_style(state, ui);
    update_title(state, ui);
    update_link_button(state, ui);
    ui.canon_map.queue_draw();
    let active = state.borrow().active;
    if let Some(pu) = ui.pane_uis.borrow().get(active) {
        pu.area.grab_focus();
    }
    // Redraw connectors once the fresh panes have painted (produced their dls).
    let ui2 = ui.clone();
    glib::timeout_add_local_once(Duration::from_millis(60), move || ui2.link_layer.queue_draw());
}

/// Build one pane column (nav strip + scrolled canvas) wired to pane `i`.
fn build_pane(state: &Shared, ui: &Ui, i: usize, n: usize) -> (gtk::Box, PaneUi) {
    // ── nav strip ──────────────────────────────────────────────────────────────
    let strip = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    strip.add_css_class("panenav");

    let book_names: Vec<&str> = canon::BOOKS.iter().map(|b| b.name).collect();
    let book_dd = gtk::DropDown::from_strings(&book_names);
    let chapter_spin = gtk::SpinButton::with_range(1.0, 150.0, 1.0);
    let prev = gtk::Button::from_icon_name("go-previous-symbolic");
    let next = gtk::Button::from_icon_name("go-next-symbolic");
    let filler = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    filler.set_hexpand(true);
    strip.append(&book_dd);
    strip.append(&chapter_spin);
    strip.append(&prev);
    strip.append(&next);
    strip.append(&filler);

    if n < MAX_PANES {
        let add = gtk::Button::from_icon_name("list-add-symbolic");
        add.set_tooltip_text(Some("Add a pane"));
        let state = state.clone();
        let ui = ui.clone();
        add.connect_clicked(move |_| add_pane(&state, &ui, i));
        strip.append(&add);
    }
    if n > 1 {
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        close.set_tooltip_text(Some("Close this pane"));
        let state = state.clone();
        let ui = ui.clone();
        close.connect_clicked(move |_| close_pane(&state, &ui, i));
        strip.append(&close);
    }

    // ── canvas ──────────────────────────────────────────────────────────────────
    let area = gtk::DrawingArea::new();
    area.set_hexpand(true);
    area.set_vexpand(true);
    area.set_focusable(true);
    area.add_css_class("scripture");

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_child(Some(&area));
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    let vadj = scroll.vadjustment();
    // Scrolling this pane moves its verses, so the connector lines must redraw.
    {
        let layer = ui.link_layer.clone();
        vadj.connect_value_changed(move |_| layer.queue_draw());
    }

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.set_hexpand(true);
    root.append(&strip);
    root.append(&scroll);

    // ── paint ─────────────────────────────────────────────────────────────────
    {
        let state = state.clone();
        area.set_draw_func(move |a, cr, w, _h| draw_pane(&state, i, a, cr, w));
    }

    // ── hover a Strong's-tagged word → quick gloss tooltip ──────────────────────
    area.set_has_tooltip(true);
    {
        let state = state.clone();
        area.connect_query_tooltip(move |_w, x, y, _kbd, tooltip| {
            let markup = {
                let st = state.borrow();
                st.panes
                    .get(i)
                    .and_then(|p| {
                        p.dl.as_ref()
                            .and_then(|dl| dl.hit_test(x as f32 - p.margin_x, y as f32 - MARGIN))
                    })
                    .filter(|hit| !hit.strongs.is_empty())
                    .map(|hit| hover_markup(&st, &hit))
            };
            match markup {
                Some(m) => {
                    tooltip.set_markup(Some(&m));
                    true
                }
                None => false,
            }
        });
    }

    // ── click: activate the pane; double-click looks up a word ──────────────────
    {
        let state = state.clone();
        let ui = ui.clone();
        let area2 = area.clone();
        let click = gtk::GestureClick::new();
        click.set_button(gdk::BUTTON_PRIMARY);
        click.connect_pressed(move |_g, n_press, x, y| {
            set_active(&state, &ui, i);
            area2.grab_focus();
            // The reader paints everything offset down by MARGIN, so undo that on
            // the y before hit-testing the display list.
            let hit = {
                let st = state.borrow();
                st.panes.get(i).and_then(|p| {
                    p.dl.as_ref()
                        .and_then(|dl| dl.hit_test(x as f32 - p.margin_x, y as f32 - MARGIN))
                })
            };
            let Some(hit) = hit else { return };
            if n_press == 2 {
                // Double-click → Strong's study.
                let markup = word_study_markup(&state.borrow(), &hit);
                show_study(&ui, &markup);
            } else {
                // Single-click → pin this word as a weave-link endpoint (click
                // another word in the same verse to grow the span).
                set_pin(&state, &ui, i, hit.verse.verse, hit.token_index);
            }
        });
        area.add_controller(click);
    }

    // ── Ctrl+scroll zooms every pane ─────────────────────────────────────────────
    {
        let state = state.clone();
        let ui = ui.clone();
        let sc = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        sc.connect_scroll(move |c, _dx, dy| {
            if c.current_event_state().contains(gdk::ModifierType::CONTROL_MASK) {
                zoom(&state, &ui, if dy < 0.0 { 1.0 } else { -1.0 });
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        area.add_controller(sc);
    }

    // ── keyboard: scroll this pane, step this pane, zoom, close panel ────────────
    {
        let state = state.clone();
        let ui = ui.clone();
        let vadj2 = vadj.clone();
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(move |_c, key, _code, mods| {
            let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
            match key {
                gdk::Key::Page_Down | gdk::Key::space => { page_adj(&vadj2, 0.9); glib::Propagation::Stop }
                gdk::Key::Page_Up => { page_adj(&vadj2, -0.9); glib::Propagation::Stop }
                gdk::Key::Home => { vadj2.set_value(vadj2.lower()); glib::Propagation::Stop }
                gdk::Key::End => {
                    vadj2.set_value(vadj2.upper() - vadj2.page_size());
                    glib::Propagation::Stop
                }
                gdk::Key::Right | gdk::Key::bracketright => { step_pane(&state, &ui, i, 1); glib::Propagation::Stop }
                gdk::Key::Left | gdk::Key::bracketleft => { step_pane(&state, &ui, i, -1); glib::Propagation::Stop }
                gdk::Key::plus | gdk::Key::equal if ctrl => { zoom(&state, &ui, 1.0); glib::Propagation::Stop }
                gdk::Key::minus if ctrl => { zoom(&state, &ui, -1.0); glib::Propagation::Stop }
                gdk::Key::Escape => { hide_study(&ui); glib::Propagation::Stop }
                _ => glib::Propagation::Proceed,
            }
        });
        area.add_controller(keys);
    }

    // ── per-pane nav widgets ─────────────────────────────────────────────────────
    {
        let state = state.clone();
        let ui = ui.clone();
        book_dd.connect_selected_notify(move |dd| {
            if ui.guard.get() {
                return;
            }
            set_active(&state, &ui, i);
            if let Some(b) = canon::BOOKS.get(dd.selected() as usize) {
                navigate_pane(&state, &ui, i, b.id, 1, None);
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
            set_active(&state, &ui, i);
            let (book, ch) = {
                let st = state.borrow();
                (st.panes[i].book.clone(), (spin.value() as u16).max(1))
            };
            navigate_pane(&state, &ui, i, &book, ch, None);
        });
    }
    {
        let state = state.clone();
        let ui = ui.clone();
        prev.connect_clicked(move |_| {
            set_active(&state, &ui, i);
            step_pane(&state, &ui, i, -1);
        });
    }
    {
        let state = state.clone();
        let ui = ui.clone();
        next.connect_clicked(move |_| {
            set_active(&state, &ui, i);
            step_pane(&state, &ui, i, 1);
        });
    }

    let pane_ui = PaneUi { root: root.clone(), area, vadj, book_dd, chapter_spin };
    (root, pane_ui)
}

/// Mark pane `i` active (drives search / study target + title + styling).
fn set_active(state: &Shared, ui: &Ui, i: usize) {
    {
        let mut st = state.borrow_mut();
        if i >= st.panes.len() {
            return;
        }
        st.active = i;
    }
    update_active_style(state, ui);
    update_title(state, ui);
    ui.canon_map.queue_draw();
}

/// Give the active pane a gold top accent (only meaningful with >1 pane).
fn update_active_style(state: &Shared, ui: &Ui) {
    let (active, n) = {
        let st = state.borrow();
        (st.active, st.panes.len())
    };
    for (j, pu) in ui.pane_uis.borrow().iter().enumerate() {
        if n > 1 && j == active {
            pu.root.add_css_class("pane-active");
        } else {
            pu.root.remove_css_class("pane-active");
        }
    }
}

/// Reflect the active pane's location in the window subtitle.
fn update_title(state: &Shared, ui: &Ui) {
    let sub = {
        let st = state.borrow();
        let i = st.active.min(st.panes.len().saturating_sub(1));
        let p = &st.panes[i];
        format!("{} {} · 1769 KJV", canon::display_name(&p.book), p.chapter)
    };
    ui.title.set_subtitle(&sub);
}

/// Push pane `i`'s book/chapter into its nav widgets (without re-entering their
/// handlers) and request a repaint.
fn sync_pane(state: &Shared, ui: &Ui, i: usize) {
    let (book_idx, chapter, count) = {
        let st = state.borrow();
        let p = &st.panes[i];
        (
            canon::book_order(&p.book).unwrap_or(0) as u32,
            p.chapter as f64,
            st.corpus.chapter_count(&p.book).max(1) as f64,
        )
    };
    let pus = ui.pane_uis.borrow();
    if let Some(pu) = pus.get(i) {
        ui.guard.set(true);
        pu.book_dd.set_selected(book_idx);
        pu.chapter_spin.set_range(1.0, count);
        pu.chapter_spin.set_value(chapter);
        ui.guard.set(false);
        pu.area.queue_draw();
    }
}

/// Point pane `i` at `book`/`chapter`, optionally scrolling to (and tinting) a
/// verse after the next paint.
fn navigate_pane(state: &Shared, ui: &Ui, i: usize, book: &str, chapter: u16, verse: Option<u16>) {
    {
        let mut st = state.borrow_mut();
        let p = &mut st.panes[i];
        p.book = book.to_string();
        p.chapter = chapter.max(1);
        p.scroll_to = verse;
        p.highlight = verse;
    }
    sync_pane(state, ui, i);
    update_title(state, ui);
    ui.link_layer.queue_draw();
    ui.canon_map.queue_draw();

    let vadj = ui.pane_uis.borrow().get(i).map(|pu| pu.vadj.clone());
    if let Some(vadj) = vadj {
        if verse.is_none() {
            vadj.set_value(vadj.lower());
        } else {
            let state = state.clone();
            let ui = ui.clone();
            glib::timeout_add_local_once(Duration::from_millis(50), move || {
                scroll_pane_pending(&state, &ui, i);
            });
        }
    }
}

/// Step pane `i`'s chapter within its book, clamped to range.
fn step_pane(state: &Shared, ui: &Ui, i: usize, delta: i32) {
    let (book, ch) = {
        let st = state.borrow();
        let p = &st.panes[i];
        let count = st.corpus.chapter_count(&p.book);
        (p.book.clone(), (p.chapter as i32 + delta).clamp(1, count.max(1) as i32) as u16)
    };
    navigate_pane(state, ui, i, &book, ch, None);
}

/// Scroll pane `i` so its pending target verse sits near the top.
fn scroll_pane_pending(state: &Shared, ui: &Ui, i: usize) {
    let y = {
        let mut st = state.borrow_mut();
        if i >= st.panes.len() {
            return;
        }
        let p = &mut st.panes[i];
        let target = match p.scroll_to.take() {
            Some(v) => v,
            None => return,
        };
        p.dl.as_ref().and_then(|dl| {
            dl.items
                .iter()
                .find(|it| matches!(it.kind, ItemKind::VerseNumber(n) if n == target))
                .map(|it| MARGIN + it.y)
        })
    };
    if let (Some(y), Some(pu)) = (y, ui.pane_uis.borrow().get(i)) {
        let v = (y as f64 - 8.0).max(pu.vadj.lower());
        pu.vadj.set_value(v.min(pu.vadj.upper() - pu.vadj.page_size()));
    }
}

/// Insert a new pane after pane `after` (a copy of its location), made active.
fn add_pane(state: &Shared, ui: &Ui, after: usize) {
    {
        let mut st = state.borrow_mut();
        if st.panes.len() >= MAX_PANES {
            return;
        }
        let (book, chapter) = {
            let p = &st.panes[after];
            (p.book.clone(), p.chapter)
        };
        let idx = (after + 1).min(st.panes.len());
        st.panes.insert(idx, Pane::new(&book, chapter));
        st.active = idx;
    }
    rebuild_panes(state, ui);
}

/// Close pane `i` (never the last one).
fn close_pane(state: &Shared, ui: &Ui, i: usize) {
    {
        let mut st = state.borrow_mut();
        if st.panes.len() <= 1 {
            return;
        }
        st.panes.remove(i);
        if st.active >= st.panes.len() {
            st.active = st.panes.len() - 1;
        }
    }
    rebuild_panes(state, ui);
}

fn zoom(state: &Shared, ui: &Ui, dir: f64) {
    {
        let mut st = state.borrow_mut();
        st.font_size = (st.font_size + dir).clamp(MIN_FONT, MAX_FONT);
    }
    for pu in ui.pane_uis.borrow().iter() {
        pu.area.queue_draw();
    }
    ui.link_layer.queue_draw();
}

/// Pin token `tok` of `verse` in pane `i` as a weave-link endpoint. Clicking a
/// second word in the same verse grows the span from the first click's anchor;
/// clicking in a different verse starts a fresh one-word pin. Refreshes the
/// band and the link button.
fn set_pin(state: &Shared, ui: &Ui, i: usize, verse: u16, tok: u32) {
    {
        let mut st = state.borrow_mut();
        if let Some(p) = st.panes.get_mut(i) {
            p.pin = Some(match p.pin {
                Some(ps) if ps.verse == verse => PinSpan {
                    verse,
                    anchor: ps.anchor,
                    lo: ps.anchor.min(tok),
                    hi: ps.anchor.max(tok),
                },
                _ => PinSpan { verse, anchor: tok, lo: tok, hi: tok },
            });
        }
    }
    if let Some(pu) = ui.pane_uis.borrow().get(i) {
        pu.area.queue_draw();
    }
    update_link_button(state, ui);
}

/// The "＋ link" button is usable once at least two panes have a pinned verse.
fn update_link_button(state: &Shared, ui: &Ui) {
    let count = state.borrow().panes.iter().filter(|p| p.pin.is_some()).count();
    ui.link_btn.set_sensitive(count >= 2);
}

/// Clear every pane's pin.
fn clear_pins(state: &Shared) {
    let mut st = state.borrow_mut();
    for p in &mut st.panes {
        p.pin = None;
    }
}

/// Repaint all panes plus the connector overlay and the canon strip.
fn redraw_all(ui: &Ui) {
    for pu in ui.pane_uis.borrow().iter() {
        pu.area.queue_draw();
    }
    ui.link_layer.queue_draw();
    ui.canon_map.queue_draw();
}

/// Re-read weaves from disk and rebuild the cross-reference index + connector
/// pairs (after authoring a link).
fn reload_weaves(state: &Shared) {
    let home = state.borrow().home.clone();
    let (weaves, _) = weave::load_weaves(&home);
    let xrefs = build_xrefs(&weaves);
    let links = build_links(&weaves);
    let mut st = state.borrow_mut();
    st.weaves = weaves;
    st.xrefs = xrefs;
    st.links = links;
}

/// Author a weave link between the first two pinned verses: prompt for a weave
/// name, create-or-append the link, reload, and redraw the connectors.
fn make_link(state: &Shared, ui: &Ui) {
    let ends: Vec<(VRef, Span)> = {
        let st = state.borrow();
        st.panes
            .iter()
            .filter_map(|p| {
                p.pin
                    .map(|ps| (VRef::new(&p.book, p.chapter, ps.verse), (ps.lo as u16, ps.hi as u16)))
            })
            .collect()
    };
    if ends.len() < 2 {
        return;
    }
    let (a, b) = (ends[0].clone(), ends[1].clone());
    let (state, ui) = (state.clone(), ui.clone());
    let title = format!("Weave {} ↔ {}", a.0.display(), b.0.display());
    prompt_name(window_of(&ui), &title, "weave name (new or existing)", move |name| {
        let res = {
            let st = state.borrow();
            weave::add_link(
                &st.home,
                &st.weaves,
                &name,
                WeaveKind::Quotation,
                canon::TOKENIZATION_VERSION,
                &now_stamp(),
                Link::canon_span(a.0.clone(), b.0.clone(), "", Some(a.1), Some(b.1)),
            )
        };
        match res {
            Ok(_) => {
                reload_weaves(&state);
                clear_pins(&state);
                redraw_all(&ui);
                update_link_button(&state, &ui);
            }
            Err(e) => show_study(&ui, &format!("<i>Could not weave: {}</i>", esc(&e.to_string()))),
        }
    });
}

fn page_adj(vadj: &gtk::Adjustment, frac: f64) {
    let step = vadj.page_size() * frac;
    let v = (vadj.value() + step).clamp(vadj.lower(), vadj.upper() - vadj.page_size());
    vadj.set_value(v);
}

/// The toplevel window a UI lives in (for parenting modal dialogs).
fn window_of(ui: &Ui) -> Option<gtk::Window> {
    ui.study.root().and_downcast::<gtk::Window>()
}

/// Current UTC time as the frozen `YYYY-MM-DDTHH:MM:SSZ` stamp study data uses.
fn now_stamp() -> String {
    glib::DateTime::now_utc()
        .ok()
        .and_then(|d| d.format("%Y-%m-%dT%H:%M:%SZ").ok())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

/// Re-read threads + tags from disk after an authoring write.
fn reload_study_data(state: &Shared) {
    let home = state.borrow().home.clone();
    let (threads, _) = thread::load_threads(&home);
    let (tags, _) = tag::load_tags(&home);
    let mut st = state.borrow_mut();
    st.threads = threads;
    st.tags = tags;
}

/// A modal name prompt; `on_ok` runs with the trimmed name if non-empty.
fn prompt_name(parent: Option<gtk::Window>, title: &str, placeholder: &str, on_ok: impl Fn(String) + 'static) {
    let win = gtk::Window::builder().title(title).modal(true).default_width(380).build();
    if let Some(p) = &parent {
        win.set_transient_for(Some(p));
    }

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 10);
    vbox.set_margin_top(16);
    vbox.set_margin_bottom(16);
    vbox.set_margin_start(16);
    vbox.set_margin_end(16);

    let entry = gtk::Entry::new();
    entry.set_placeholder_text(Some(placeholder));

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    let ok = gtk::Button::with_label("OK");
    ok.add_css_class("suggested-action");
    buttons.append(&cancel);
    buttons.append(&ok);

    vbox.append(&entry);
    vbox.append(&buttons);
    win.set_child(Some(&vbox));

    let on_ok = Rc::new(on_ok);
    let submit = {
        let win = win.clone();
        let entry = entry.clone();
        move || {
            let text = entry.text().trim().to_string();
            win.close();
            if !text.is_empty() {
                on_ok(text);
            }
        }
    };
    {
        let submit = submit.clone();
        ok.connect_clicked(move |_| submit());
    }
    {
        let submit = submit.clone();
        entry.connect_activate(move |_| submit());
    }
    {
        let win = win.clone();
        cancel.connect_clicked(move |_| win.close());
    }

    win.present();
    entry.grab_focus();
}

/// Like [`prompt_name`] but pre-filled with `initial` and allowing an empty
/// submission (so a note can be cleared). Used for editing free-text notes.
fn prompt_text(
    parent: Option<gtk::Window>,
    title: &str,
    placeholder: &str,
    initial: &str,
    on_ok: impl Fn(String) + 'static,
) {
    let win = gtk::Window::builder().title(title).modal(true).default_width(420).build();
    if let Some(p) = &parent {
        win.set_transient_for(Some(p));
    }

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 10);
    vbox.set_margin_top(16);
    vbox.set_margin_bottom(16);
    vbox.set_margin_start(16);
    vbox.set_margin_end(16);

    let entry = gtk::Entry::new();
    entry.set_placeholder_text(Some(placeholder));
    entry.set_text(initial);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    let ok = gtk::Button::with_label("Save");
    ok.add_css_class("suggested-action");
    buttons.append(&cancel);
    buttons.append(&ok);

    vbox.append(&entry);
    vbox.append(&buttons);
    win.set_child(Some(&vbox));

    let on_ok = Rc::new(on_ok);
    let submit = {
        let win = win.clone();
        let entry = entry.clone();
        move || {
            let text = entry.text().trim().to_string();
            win.close();
            on_ok(text); // empty is allowed — it clears the note
        }
    };
    {
        let submit = submit.clone();
        ok.connect_clicked(move |_| submit());
    }
    {
        let submit = submit.clone();
        entry.connect_activate(move |_| submit());
    }
    {
        let win = win.clone();
        cancel.connect_clicked(move |_| win.close());
    }

    win.present();
    entry.grab_focus();
}

/// Add `vref` (as a whole-verse target) to the tag named `name`, then show it.
fn add_verse_to_tag(state: &Shared, ui: &Ui, vref: &VRef, name: &str) {
    let res = {
        let st = state.borrow();
        tag::add_member(
            &st.home,
            &st.tags,
            name,
            canon::TOKENIZATION_VERSION,
            TagTarget::Verse(vref.clone()),
            None,
            &now_stamp(),
        )
    };
    match res {
        Ok(_) => {
            reload_study_data(state);
            let m = {
                let st = state.borrow();
                st.tags
                    .iter()
                    .position(|lt| lt.tag.name.eq_ignore_ascii_case(name.trim()))
                    .map(|i| tag_markup(&st, i))
            };
            if let Some(m) = m {
                show_study(ui, &m);
            }
        }
        Err(e) => show_study(ui, &format!("<i>Could not tag: {}</i>", esc(&e.to_string()))),
    }
}

/// Add `vref` as a whole-verse entry (snapshotting its words) to the thread
/// named `name`, then show it.
fn add_verse_to_thread(state: &Shared, ui: &Ui, vref: &VRef, name: &str) {
    let entry = {
        let st = state.borrow();
        let (span, text) = match st.corpus.verse(vref) {
            Some(v) => {
                let words: Vec<String> = v.tokens.iter().map(|t| t.word.clone()).collect();
                let last = words.len().saturating_sub(1) as u16;
                ((0u16, last), words)
            }
            None => ((0, 0), Vec::new()),
        };
        thread::ThreadEntry { vref: vref.clone(), span, text, note: None, added: now_stamp() }
    };
    let res = {
        let st = state.borrow();
        thread::add_to_thread(&st.home, &st.threads, name, canon::TOKENIZATION_VERSION, entry)
    };
    match res {
        Ok(_) => {
            reload_study_data(state);
            let m = {
                let st = state.borrow();
                st.threads
                    .iter()
                    .position(|lt| lt.thread.name.eq_ignore_ascii_case(name.trim()))
                    .map(|i| thread_markup(&st, i))
            };
            if let Some(m) = m {
                show_study(ui, &m);
            }
        }
        Err(e) => show_study(ui, &format!("<i>Could not add to thread: {}</i>", esc(&e.to_string()))),
    }
}

/// Remove `vref` from tag `i`, then re-show that tag.
fn untag_verse(state: &Shared, ui: &Ui, i: usize, vref: &VRef) {
    let removed = {
        let st = state.borrow();
        st.tags.get(i).map(|lt| tag::remove_member(lt, &TagTarget::Verse(vref.clone())))
    };
    if let Some(Ok(())) = removed {
        reload_study_data(state);
    }
    let m = {
        let st = state.borrow();
        st.tags.get(i).map(|_| tag_markup(&st, i))
    };
    if let Some(m) = m {
        show_study(ui, &m);
    }
}

/// Parse a study-panel link: `go:Book:ch[:verse]` navigates the active pane,
/// `occ:CODE` opens that Strong's concordance, `thread:`/`tag:` open a study
/// collection, and `addthread:`/`addtag:`/`untag:` author study data.
fn handle_link(state: &Shared, ui: &Ui, uri: &str) {
    if let Some(rest) = uri.strip_prefix("go:") {
        let active = state.borrow().active;
        let parts: Vec<&str> = rest.split(':').collect();
        if let [book, ch] = parts[..] {
            if let Ok(c) = ch.parse::<u16>() {
                navigate_pane(state, ui, active, book, c, None);
            }
        } else if let [book, ch, v] = parts[..] {
            if let (Ok(c), Ok(v)) = (ch.parse::<u16>(), v.parse::<u16>()) {
                navigate_pane(state, ui, active, book, c, Some(v));
            }
        }
    } else if let Some(code) = uri.strip_prefix("occ:") {
        let markup = concordance_markup(&state.borrow(), code);
        show_study(ui, &markup);
    } else if let Some(idx) = uri.strip_prefix("thread:") {
        if let Ok(i) = idx.parse::<usize>() {
            let markup = thread_markup(&state.borrow(), i);
            show_study(ui, &markup);
        }
    } else if let Some(idx) = uri.strip_prefix("tag:") {
        if let Ok(i) = idx.parse::<usize>() {
            let markup = tag_markup(&state.borrow(), i);
            show_study(ui, &markup);
        }
    } else if let Some(rk) = uri.strip_prefix("addtag:") {
        if let Some(vref) = VRef::parse_ref_key(rk) {
            let (state, ui) = (state.clone(), ui.clone());
            let title = format!("Tag {}", vref.display());
            prompt_name(window_of(&ui), &title, "tag name (new or existing)", move |name| {
                add_verse_to_tag(&state, &ui, &vref, &name);
            });
        }
    } else if let Some(rk) = uri.strip_prefix("addthread:") {
        if let Some(vref) = VRef::parse_ref_key(rk) {
            let (state, ui) = (state.clone(), ui.clone());
            let title = format!("Add {} to thread", vref.display());
            prompt_name(window_of(&ui), &title, "thread name (new or existing)", move |name| {
                add_verse_to_thread(&state, &ui, &vref, &name);
            });
        }
    } else if let Some(rest) = uri.strip_prefix("untag:") {
        if let Some((idx, rk)) = rest.split_once(':') {
            if let (Ok(i), Some(vref)) = (idx.parse::<usize>(), VRef::parse_ref_key(rk)) {
                untag_verse(state, ui, i, &vref);
            }
        }
    } else if let Some(idx) = uri.strip_prefix("approve:") {
        if let Ok(i) = idx.parse::<usize>() {
            review_weave(state, ui, i, true);
        }
    } else if let Some(idx) = uri.strip_prefix("reject:") {
        if let Ok(i) = idx.parse::<usize>() {
            review_weave(state, ui, i, false);
        }
    } else if let Some(idx) = uri.strip_prefix("editthreadnotes:") {
        if let Ok(i) = idx.parse::<usize>() {
            edit_thread_notes(state, ui, i);
        }
    } else if let Some(rest) = uri.strip_prefix("editentrynote:") {
        if let Some((ti, ei)) = rest.split_once(':') {
            if let (Ok(ti), Ok(ei)) = (ti.parse::<usize>(), ei.parse::<usize>()) {
                edit_entry_note(state, ui, ti, ei);
            }
        }
    } else if let Some(idx) = uri.strip_prefix("editweavenotes:") {
        if let Ok(i) = idx.parse::<usize>() {
            edit_weave_notes(state, ui, i);
        }
    } else if let Some(idx) = uri.strip_prefix("weave:") {
        if let Ok(i) = idx.parse::<usize>() {
            let m = weave_markup(&state.borrow(), i);
            show_study(ui, &m);
        }
    }
}

/// Prompt for and save the running notes document of thread `i`.
fn edit_thread_notes(state: &Shared, ui: &Ui, i: usize) {
    let (name, current) = {
        let st = state.borrow();
        let Some(lt) = st.threads.get(i) else { return };
        (lt.thread.name.clone(), lt.thread.notes.clone())
    };
    let (state, ui) = (state.clone(), ui.clone());
    prompt_text(window_of(&ui), &format!("Notes — {name}"), "thread notes", &current, move |notes| {
        let res = { thread::set_thread_notes(&state.borrow().threads, &name, &notes) };
        finish_note_edit(&state, &ui, res, i);
    });
}

/// Prompt for and save the note on entry `ei` of thread `ti`.
fn edit_entry_note(state: &Shared, ui: &Ui, ti: usize, ei: usize) {
    let (name, current) = {
        let st = state.borrow();
        let Some(lt) = st.threads.get(ti) else { return };
        let cur = lt.thread.entries.get(ei).and_then(|e| e.note.clone()).unwrap_or_default();
        (lt.thread.name.clone(), cur)
    };
    let (state, ui) = (state.clone(), ui.clone());
    prompt_text(window_of(&ui), "Entry note", "note (empty clears it)", &current, move |note| {
        let res = { thread::set_entry_note(&state.borrow().threads, &name, ei, Some(note)) };
        finish_note_edit(&state, &ui, res, ti);
    });
}

/// Prompt for and save the notes document of the weave at global index `i`.
fn edit_weave_notes(state: &Shared, ui: &Ui, i: usize) {
    let (name, current) = {
        let st = state.borrow();
        let Some(lw) = st.weaves.get(i) else { return };
        (lw.weave.name.clone(), lw.weave.notes.clone())
    };
    let (state, ui) = (state.clone(), ui.clone());
    prompt_text(window_of(&ui), &format!("Notes — {name}"), "weave notes", &current, move |notes| {
        let res = { weave::set_weave_notes(&state.borrow().weaves, &name, &notes) };
        match res {
            Ok(_) => {
                reload_weaves(&state);
                let m = suggested_list_markup(&state.borrow());
                show_study(&ui, &m);
            }
            Err(e) => show_study(&ui, &format!("<i>Could not save notes: {}</i>", esc(&e.to_string()))),
        }
    });
}

/// Shared tail for thread note edits: reload study data and re-show the thread.
fn finish_note_edit(state: &Shared, ui: &Ui, res: Result<std::path::PathBuf, pure_core::Error>, thread_idx: usize) {
    match res {
        Ok(_) => {
            reload_study_data(state);
            let m = thread_markup(&state.borrow(), thread_idx);
            show_study(ui, &m);
        }
        Err(e) => show_study(ui, &format!("<i>Could not save note: {}</i>", esc(&e.to_string()))),
    }
}

/// Approve (promote to `weaves/`, all links approved) or reject (delete) the
/// weave at index `i` in `st.weaves`, then reload weaves and re-show the
/// suggested list. All writes go through the cross-platform `core::store`.
fn review_weave(state: &Shared, ui: &Ui, i: usize, approve: bool) {
    let result = {
        let st = state.borrow();
        let Some(lw) = st.weaves.get(i) else { return };
        if approve {
            weave::approve_weave(&st.home, lw).map(|_| ())
        } else {
            weave::reject_weave(lw)
        }
    };
    match result {
        Ok(()) => {
            reload_weaves(state);
            let m = suggested_list_markup(&state.borrow());
            show_study(ui, &m);
        }
        Err(e) => show_study(
            ui,
            &format!(
                "<i>Could not {} weave: {}</i>",
                if approve { "approve" } else { "reject" },
                esc(&e.to_string())
            ),
        ),
    }
}

/// Lay out and paint pane `i`'s chapter. Measurement and painting share one
/// Pango layout, so the stored hit regions match the glyphs exactly.
fn draw_pane(state: &Shared, i: usize, area: &gtk::DrawingArea, cr: &cairo::Context, width: i32) {
    // warm paper background
    cr.set_source_rgb(0.988, 0.976, 0.957);
    let _ = cr.paint();

    let mut st = state.borrow_mut();
    if i >= st.panes.len() {
        return;
    }
    let family = st.family.clone();
    let font_size = st.font_size;
    let book = st.panes[i].book.clone();
    let chapter = st.panes[i].chapter;
    let highlight = st.panes[i].highlight;
    let pin = st.panes[i].pin;

    // One Pango layout, reused to measure then paint; three font variants.
    let layout = pangocairo::functions::create_layout(cr);
    let mut regular = pango::FontDescription::new();
    regular.set_family(&family);
    regular.set_absolute_size(font_size * pango::SCALE as f64);
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

    let cfg = LayoutConfig {
        width: col,
        line_height,
        space_width,
        verse_num_gap: space_width * 1.4,
        para_indent: line_height * 0.9,
        para_spacing: line_height * 0.45,
    };

    let verses = st.corpus.chapter_verses(&book, chapter).to_vec();
    let measure = PangoMeasure { layout: &layout };
    let dl = layout_chapter(&verses, &measure, &cfg);
    let top = MARGIN;

    // Verses in this chapter carrying weave cross-references get a gutter dot.
    let xref_here: HashSet<u16> = verses
        .iter()
        .map(|v| v.verse)
        .filter(|&n| st.xrefs.contains_key(&VRef::new(&book, chapter, n)))
        .collect();

    // Soft band behind a verse: gold for a search/cross-ref target, blue for a
    // pinned weave-link endpoint.
    let band = |cr: &cairo::Context, verse: u16, r: f64, g: f64, b: f64, a: f64| {
        let ys: Vec<f32> = dl
            .items
            .iter()
            .filter(|it| item_verse_num(it) == Some(verse))
            .map(|it| it.y)
            .collect();
        if let (Some(&y0), Some(&y1)) = (ys.iter().next(), ys.iter().last()) {
            cr.set_source_rgba(r, g, b, a);
            let _ = cr.rectangle(
                margin_x as f64 - 6.0,
                top as f64 + y0 as f64,
                col as f64 + 12.0,
                (y1 - y0) as f64 + line_height as f64,
            );
            let _ = cr.fill();
        }
    };
    if let Some(hv) = highlight {
        band(cr, hv, 0.62, 0.49, 0.22, 0.12);
    }
    // A pinned endpoint highlights its exact word span (blue), so the reader
    // sees which words the link will point at — not just the whole verse.
    if let Some(ps) = pin {
        let pv = VRef::new(&book, chapter, ps.verse);
        cr.set_source_rgba(0.25, 0.45, 0.75, 0.22);
        for it in &dl.items {
            if let Some((v, t)) = it.word() {
                if *v == pv && t >= ps.lo && t <= ps.hi {
                    let _ = cr.rectangle(
                        (margin_x + it.x) as f64 - 1.5,
                        top as f64 + it.y as f64,
                        it.w as f64 + 3.0,
                        it.h as f64,
                    );
                    let _ = cr.fill();
                }
            }
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
    let pane = &mut st.panes[i];
    pane.margin_x = margin_x;
    pane.dl = Some(dl);
    if pane.last_h != content_height {
        pane.last_h = content_height;
        area.set_content_height(content_height);
    }
}

/// Draw the ambient weave connectors: for every link whose both endpoints are
/// currently visible (their chapters shown in some pane, their verses on
/// screen), a soft gold curve from one to the other. `compute_point` maps each
/// verse's content position — scroll and pane offset included — into this layer.
fn draw_links(state: &Shared, ui: &Ui, layer: &gtk::DrawingArea, cr: &cairo::Context, _w: i32, h: i32) {
    let st = state.borrow();
    let pus = ui.pane_uis.borrow();
    // Which shown (book, chapter) sits in which pane.
    let mut shown: HashMap<(&str, u16), usize> = HashMap::new();
    for (i, p) in st.panes.iter().enumerate() {
        shown.insert((p.book.as_str(), p.chapter), i);
    }

    cr.set_line_width(1.5);
    for (a, b) in &st.links {
        let (Some(&pa), Some(&pb)) = (
            shown.get(&(a.book.as_str(), a.chapter)),
            shown.get(&(b.book.as_str(), b.chapter)),
        ) else {
            continue;
        };
        let (Some(ea), Some(eb)) = (
            link_endpoint(&st, &pus, pa, a.verse, layer),
            link_endpoint(&st, &pus, pb, b.verse, layer),
        ) else {
            continue;
        };
        // Both endpoints must be within the visible band (not scrolled off).
        let vis = |y: f64| (0.0..=h as f64).contains(&y);
        if !vis(ea.1) || !vis(eb.1) {
            continue;
        }
        let dx = eb.0 - ea.0;
        cr.set_source_rgba(0.62, 0.49, 0.22, 0.35);
        cr.move_to(ea.0, ea.1);
        cr.curve_to(ea.0 + dx * 0.4, ea.1, eb.0 - dx * 0.4, eb.1, eb.0, eb.1);
        let _ = cr.stroke();
        for (x, y) in [ea, eb] {
            cr.set_source_rgba(0.62, 0.49, 0.22, 0.7);
            cr.arc(x, y, 2.0, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
        }
    }
}

/// The position, in `layer` coordinates, of `verse`'s line in pane `pane` — or
/// `None` if the pane hasn't painted or lacks that verse.
fn link_endpoint(
    st: &State,
    pus: &[PaneUi],
    pane: usize,
    verse: u16,
    layer: &gtk::DrawingArea,
) -> Option<(f64, f64)> {
    let p = st.panes.get(pane)?;
    let dl = p.dl.as_ref()?;
    let it = dl
        .items
        .iter()
        .find(|it| matches!(it.kind, ItemKind::VerseNumber(n) if n == verse))?;
    let px = p.margin_x + it.x;
    let py = MARGIN + it.y + it.h * 0.5;
    let pt = pus
        .get(pane)?
        .area
        .compute_point(layer, &gtk::graphene::Point::new(px, py))?;
    Some((pt.x() as f64, pt.y() as f64))
}

/// Draw the canon-overview strip: the eight canon sections across the 66 books,
/// the OT/NT divide, and a pin per pane at its current book (active in gold).
fn draw_canon(state: &Shared, cr: &cairo::Context, w: i32, h: i32) {
    let st = state.borrow();
    let width = w as f64;
    let hf = h as f64;
    let nb = canon::BOOKS.len() as f64;

    cr.set_source_rgb(0.92, 0.90, 0.86);
    let _ = cr.rectangle(0.0, 0.0, width, hf);
    let _ = cr.fill();

    let layout = pangocairo::functions::create_layout(cr);
    let mut fd = pango::FontDescription::new();
    fd.set_family(&st.family);
    fd.set_absolute_size(11.0 * pango::SCALE as f64);
    layout.set_font_description(Some(&fd));

    for (k, (lbl, lo, hi)) in CANON_SEGMENTS.iter().enumerate() {
        let x0 = *lo as f64 / nb * width;
        let x1 = (*hi + 1) as f64 / nb * width;
        if k % 2 == 1 {
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.04);
            let _ = cr.rectangle(x0, 0.0, x1 - x0, hf);
            let _ = cr.fill();
        }
        layout.set_text(lbl);
        let (tw, th) = layout.pixel_size();
        if (tw as f64) < (x1 - x0) - 6.0 {
            cr.set_source_rgba(0.35, 0.30, 0.22, 0.9);
            cr.move_to((x0 + x1) / 2.0 - tw as f64 / 2.0, hf / 2.0 - th as f64 / 2.0 - 2.0);
            pangocairo::functions::show_layout(cr, &layout);
        }
    }

    let dx = OT_NT_DIVIDE as f64 / nb * width;
    cr.set_source_rgba(0.4, 0.3, 0.2, 0.5);
    cr.set_line_width(1.0);
    cr.move_to(dx, 0.0);
    cr.line_to(dx, hf);
    let _ = cr.stroke();

    for (i, p) in st.panes.iter().enumerate() {
        let bi = canon::book_order(&p.book).unwrap_or(0) as f64;
        let x = (bi + 0.5) / nb * width;
        if i == st.active {
            cr.set_source_rgb(0.62, 0.49, 0.22);
        } else {
            cr.set_source_rgba(0.3, 0.3, 0.3, 0.6);
        }
        cr.arc(x, hf - 4.0, 3.5, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
    }
}

/// The verse number a placed item belongs to (its own number for a marker; for
/// a word, from its `VRef`). Used to band the highlighted verse.
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

/// Markup for a clicked word: its Strong's entries (each with a concordance
/// link), this verse's weave cross-references, and its 1769 margin notes.
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

    // Author actions on this verse.
    let rk = esc(&hit.verse.ref_key());
    s.push_str(&format!(
        "\n<a href=\"addtag:{rk}\">＋ tag verse</a>    <a href=\"addthread:{rk}\">＋ add to thread</a>\n"
    ));

    // Weave cross-references touching this verse, if any.
    if let Some(xs) = st.xrefs.get(&hit.verse) {
        s.push_str(&format!("\n<b>cross-references ({})</b>\n", xs.len()));
        for x in xs.iter().take(XREF_SHOWN) {
            // Link the weave name to its compare card (first weave of that name).
            let weave_link = match st.weaves.iter().position(|lw| lw.weave.name == x.weave) {
                Some(wi) => format!("<a href=\"weave:{wi}\">{}</a>", esc(&x.weave)),
                None => esc(&x.weave),
            };
            s.push_str(&format!(
                "{}  <small><span foreground=\"#888\">{}</span></small>\n",
                go_link(&x.partner),
                weave_link
            ));
        }
        if xs.len() > XREF_SHOWN {
            s.push_str(&format!("<small>… {} more</small>\n", xs.len() - XREF_SHOWN));
        }
    }

    // Tags that include this verse, if any.
    let vt = TagTarget::Verse(hit.verse.clone());
    let tagged: Vec<(usize, &LoadedTag)> =
        st.tags.iter().enumerate().filter(|(_, lt)| lt.tag.member_of(&vt)).collect();
    if !tagged.is_empty() {
        s.push_str("\n<b>tags</b>\n");
        for (i, lt) in tagged {
            s.push_str(&format!(
                "<a href=\"tag:{i}\">{}</a>  <a href=\"untag:{i}:{rk}\"><small>✕</small></a>\n",
                esc(&lt.tag.name)
            ));
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

/// The list of threads, each a link that opens its passages.
fn threads_list_markup(st: &State) -> String {
    if st.threads.is_empty() {
        return "<b>Threads</b>\n\n<i>None yet — a thread is an ordered trail of \
                passages you gather (authoring is coming).</i>"
            .to_string();
    }
    let mut s = format!("<b>Threads ({})</b>\n\n", st.threads.len());
    for (i, lt) in st.threads.iter().enumerate() {
        let n = lt.thread.entries.len();
        s.push_str(&format!(
            "<a href=\"thread:{}\">{}</a>  <small>{} passage{}</small>\n",
            i,
            esc(&lt.thread.name),
            n,
            if n == 1 { "" } else { "s" }
        ));
    }
    s
}

/// One thread: its passages as jump links with a snapshot preview + note.
fn thread_markup(st: &State, i: usize) -> String {
    let Some(lt) = st.threads.get(i) else {
        return String::new();
    };
    let t = &lt.thread;
    let mut s = format!("<b>{}</b>\n", esc(&t.name));
    if !t.notes.is_empty() {
        s.push_str(&format!("<small>{}</small>\n", esc(&t.notes)));
    }
    s.push_str(&format!(
        "<small>{} passages</small>  <a href=\"editthreadnotes:{i}\"><small>✎ notes</small></a>\n\n",
        t.entries.len()
    ));
    for (j, e) in t.entries.iter().enumerate() {
        s.push_str(&go_link(&e.vref));
        let snippet = e.text.join(" ");
        let short: String = snippet.chars().take(70).collect();
        let ell = if snippet.chars().count() > 70 { "…" } else { "" };
        s.push_str(&format!("\n<small>{}{}</small>\n", esc(&short), ell));
        if let Some(n) = &e.note {
            s.push_str(&format!("<small><span foreground=\"#888\">— {}</span></small>\n", esc(n)));
        }
        s.push_str(&format!(
            "<a href=\"editentrynote:{i}:{j}\"><small>✎ note</small></a>\n\n"
        ));
    }
    s
}

/// The list of tags, each a link that opens its members.
fn tags_list_markup(st: &State) -> String {
    if st.tags.is_empty() {
        return "<b>Tags</b>\n\n<i>None yet — a tag groups verses and Strong's \
                concepts under a label (authoring is coming).</i>"
            .to_string();
    }
    let mut s = format!("<b>Tags ({})</b>\n\n", st.tags.len());
    for (i, lt) in st.tags.iter().enumerate() {
        let n = lt.tag.members.len();
        s.push_str(&format!(
            "<a href=\"tag:{}\">{}</a>  <small>{} member{}</small>\n",
            i,
            esc(&lt.tag.name),
            n,
            if n == 1 { "" } else { "s" }
        ));
    }
    s
}

/// One tag: its members — verses as jump links, concepts as concordance links.
fn tag_markup(st: &State, i: usize) -> String {
    let Some(lt) = st.tags.get(i) else {
        return String::new();
    };
    let t = &lt.tag;
    let mut s = format!("<b>{}</b>\n<small>{} members</small>\n\n", esc(&t.name), t.members.len());
    for m in &t.members {
        match &m.target {
            TagTarget::Verse(v) => s.push_str(&go_link(v)),
            TagTarget::Concept(c) => {
                s.push_str(&format!("<a href=\"occ:{}\">{}</a>", esc(c), esc(c)))
            }
        }
        if let Some(n) = &m.note {
            s.push_str(&format!("  <small><span foreground=\"#888\">{}</span></small>", esc(n)));
        }
        s.push('\n');
    }
    s
}

/// The suggested weaves awaiting review, each with its links and
/// approve/reject actions. Weaves are addressed by their index in `st.weaves`
/// (the flat list of canonical + suggested), so the action links stay valid
/// until the next reload.
fn suggested_list_markup(st: &State) -> String {
    let suggested: Vec<(usize, &LoadedWeave)> =
        st.weaves.iter().enumerate().filter(|(_, lw)| weave::is_suggested(lw)).collect();
    if suggested.is_empty() {
        return "<b>Suggested weaves</b>\n\n<i>None waiting — proposed weaves \
                (dropped in <tt>weaves/suggested</tt>) appear here for you to \
                approve or reject.</i>"
            .to_string();
    }
    let mut s = format!("<b>Suggested weaves ({})</b>\n\n", suggested.len());
    for (i, lw) in suggested {
        let w = &lw.weave;
        s.push_str(&format!(
            "<b>{}</b>  <small><span foreground=\"#888\">{}</span></small>\n",
            esc(&w.name),
            esc(w.kind.label())
        ));
        if !w.notes.is_empty() {
            s.push_str(&format!("<small>{}</small>\n", esc(&w.notes)));
        }
        for l in w.links.iter().take(XREF_SHOWN) {
            s.push_str(&format!("<small>{} ↔ {}</small>\n", go_link(&l.a), go_link(&l.b)));
        }
        if w.links.len() > XREF_SHOWN {
            s.push_str(&format!("<small>… {} more links</small>\n", w.links.len() - XREF_SHOWN));
        }
        s.push_str(&format!(
            "<a href=\"weave:{i}\">⇔ compare</a>    <a href=\"approve:{i}\">✓ approve</a>    <a href=\"reject:{i}\">✕ reject</a>    <a href=\"editweavenotes:{i}\">✎ note</a>\n\n"
        ));
    }
    s
}

/// A verse's words as markup, with the tokens in `span` (inclusive, if any)
/// emphasized (bold). Words the KJV translators supplied stay italic-gray, as
/// in the reader. Falls back to the ref key if the verse is absent.
fn verse_text_spanned(st: &State, vref: &VRef, span: Option<Span>) -> String {
    let Some(v) = st.corpus.verse(vref) else {
        return esc(&vref.ref_key());
    };
    let mut s = String::new();
    for (k, t) in v.tokens.iter().enumerate() {
        if k > 0 {
            s.push(' ');
        }
        let in_span = span.is_some_and(|(lo, hi)| k as u16 >= lo && k as u16 <= hi);
        let word = esc(&t.word);
        let word = if t.flags & FLAG_ADDED != 0 {
            format!("<span foreground=\"#6b6862\"><i>{word}</i></span>")
        } else {
            word
        };
        if in_span {
            s.push_str(&format!("<b>{word}</b>"));
        } else {
            s.push_str(&word);
        }
    }
    s
}

/// The weave compare card: the weave's kind + notes, then each link as its two
/// linked passages one above the other, the linked words emphasized, with jump
/// links and an "✎ note" editor. `i` is the global index into `st.weaves`.
fn weave_markup(st: &State, i: usize) -> String {
    let Some(lw) = st.weaves.get(i) else {
        return String::new();
    };
    let w = &lw.weave;
    let tag = if weave::is_suggested(lw) { "  <small><span foreground=\"#888\">(suggested)</span></small>" } else { "" };
    let mut s = format!(
        "<b>{}</b>  <small><span foreground=\"#888\">{}</span></small>{}\n",
        esc(&w.name),
        esc(w.kind.label()),
        tag
    );
    s.push_str(&format!(
        "<small>{} link{}</small>  <a href=\"editweavenotes:{i}\"><small>✎ note</small></a>\n",
        w.links.len(),
        if w.links.len() == 1 { "" } else { "s" }
    ));
    if !w.notes.is_empty() {
        s.push_str(&format!("<small>{}</small>\n", esc(&w.notes)));
    }
    s.push('\n');
    for l in w.links.iter().take(XREF_SHOWN) {
        if !l.label.is_empty() {
            s.push_str(&format!("<small><span foreground=\"#9e7d38\">“{}”</span></small>\n", esc(&l.label)));
        }
        s.push_str(&format!("{}\n<small>{}</small>\n", go_link(&l.a), verse_text_spanned(st, &l.a, l.span_a)));
        s.push_str(&format!("{}\n<small>{}</small>\n\n", go_link(&l.b), verse_text_spanned(st, &l.b, l.span_b)));
    }
    if w.links.len() > XREF_SHOWN {
        s.push_str(&format!("<small>… {} more links</small>\n", w.links.len() - XREF_SHOWN));
    }
    s
}

/// A compact hover gloss for a Strong's-tagged word: each code with its lemma,
/// transliteration, and a trimmed definition.
fn hover_markup(st: &State, hit: &Hit) -> String {
    let mut s = String::new();
    for (k, code) in hit.strongs.iter().enumerate() {
        if k > 0 {
            s.push('\n');
        }
        s.push_str(&format!("<b>{}</b>", esc(code)));
        if let Some(e) = st.strongs.get(code) {
            if let Some(l) = &e.lemma {
                s.push_str(&format!("  {}", esc(l)));
            }
            if let Some(x) = &e.xlit {
                s.push_str(&format!("  <i>{}</i>", esc(x)));
            }
            if let Some(g) = e.kjv.as_ref().or(e.def.as_ref()) {
                let g = g.trim();
                let short: String = g.chars().take(80).collect();
                let ell = if g.chars().count() > 80 { "…" } else { "" };
                s.push_str(&format!("\n<small>{}{}</small>", esc(&short), ell));
            }
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
         label { font-family: \"EB Garamond\", serif; } \
         .panenav { background: #efeae1; padding: 3px; } \
         box.pane-active { border-top: 2px solid #9e7d38; }",
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
