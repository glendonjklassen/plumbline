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
use gtk::{cairo, gdk, gio, glib};

use pure_core::config::{self, Config, StudyMode};
use pure_core::corpus::{Corpus, FLAG_ADDED, FLAG_DIVINE, FLAG_TITLE};
use pure_core::search::{self, Notes, SearchAnswer, SearchIx};
use pure_core::strongs::{self, OccurrenceIx, StrongsDict};
use pure_core::renderings::Renderings;
use pure_core::reference::{CANON_SEGMENTS, OT_NT_DIVIDE};
use pure_core::tag::{self, LoadedTag, TagTarget};
use pure_core::thread::{self, LoadedThread};
use pure_core::panel::{self, PanelSource};
use pure_core::weave::{self, Link, LoadedWeave, Span, WeaveKind};
use pure_core::{canon, corpus, crossref, export, home, notes, theme, usernote, VRef};
use pure_rnd::{bridge, burst, concept, embed, morph, witness};
use pure_layout::{layout_chapter, DisplayList, Hit, ItemKind, LayoutConfig, Measure};

const APP_ID: &str = "dev.purestudy.app";
const MAX_COLUMN: f32 = 720.0;
const MARGIN: f32 = 28.0;
const MIN_FONT: f64 = 12.0;
const MAX_FONT: f64 = 48.0;
/// The default body size (matches `config::Config::default().body_size`); what
/// Ctrl+0 resets the zoom to.
const DEFAULT_FONT: f64 = 18.0;
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
    /// The rendering lens: code → English renderings and word → codes.
    renderings: Renderings,
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
    /// The reader's personal per-verse notes (Tier 0 #3), keyed by verse.
    usernotes: HashMap<VRef, usernote::LoadedNote>,
    /// Every current search hit — banded in whatever chapter shows them (#8).
    hits: HashSet<VRef>,
    /// TSK topical cross-references per verse (empty when the file is absent).
    xref_ix: crossref::XRefIx,
    /// The OT↔NT bridge: Strong's etymology fused with external witnesses (LXX,
    /// Abbott-Smith, TIPNR) weighted by trust priors. A Full-study R&D tier.
    bridge: bridge::FusedBridge,
    /// The graded text-as-witness — can flag a bridge link it disbelieves (only
    /// once it has passed grading; silent otherwise).
    witness: witness::TextWitness,
    /// Concept embeddings (offline-trained), when the artifact is present — for
    /// "concepts near this" and the cross-testament semantic bridge.
    embedding: Option<embed::Embedding>,
    /// Morphology sidecar (offline-projected), when present — per-token parse.
    morph: Option<morph::MorphData>,
    /// Heavy R&D analytics, built lazily on the first Full-study word lookup
    /// (each is a corpus-wide sweep) and cached — so startup stays instant and
    /// a Simple reader never pays for them. See `ensure_analytics`.
    /// SIF verse-similarity model over the embedding — "verses like this".
    verse_sim: Option<embed::VerseSim>,
    /// Symbolic concept engine: collocation communities + book distribution.
    concept: Option<concept::Concept>,
    /// Discovered leitwörter (bursty concepts) keyed by Strong's code.
    leitwort: Option<HashMap<String, burst::Burst>>,
    /// Whether the lazy analytics above have been built this session.
    analytics_built: bool,
    /// The data home, kept so authoring can write + reload study data.
    home: String,
    /// Font family to render the scripture in ("EB Garamond" or a fallback).
    family: String,
    font_size: f64,
    /// Simple reader vs full study — gates the study/authoring surface
    /// (decision #4). Persisted in the platform config.
    mode: StudyMode,
    /// Verse-per-line reading mode (each verse starts a fresh line).
    verse_per_line: bool,
    /// The active colour theme + the user's choice (Tier 0 #5).
    palette: theme::Palette,
    theme_choice: theme::ThemeChoice,
    /// Set true by a capture-phase click on a panel link when a modifier is
    /// held, so the next `go:` opens in the other pane (Tier 0 #8).
    link_other: bool,
    panes: Vec<Pane>,
    /// Which pane search / cross-references / the study panel act on.
    active: usize,
}

/// One reading column: what it shows plus its last paint (for hit-testing and
/// scroll-to-verse).
/// A drag must move at least this many pixels before it counts as a highlight
/// selection rather than a click-to-pin (Tier 0 #4).
const HL_DRAG_THRESHOLD: f64 = 6.0;

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
    /// A live cross-verse highlight drag (Tier 0 #4): (startRef, startTok) →
    /// (endRef, endTok), for the drag preview; set past the threshold, cleared
    /// on release.
    hl_drag: Option<(VRef, u32, VRef, u32)>,
    /// Per-pane reading history (Tier 0 #2): visited (book, chapter) + a cursor.
    history: Vec<(String, u16)>,
    hist_idx: isize,
    in_history_nav: bool,
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
            hl_drag: None,
            // Seed the history with the opening chapter so the first "back" works.
            history: vec![(book.to_string(), chapter)],
            hist_idx: 0,
            in_history_nav: false,
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
    /// The split between the reading panes and the study panel.
    paned: gtk::Paned,
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
    /// The app CSS provider, re-loaded on a theme switch (Tier 0 #5).
    css: gtk::CssProvider,
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

/// The study panel's target width when open — a stable sidebar, so it never
/// balloons to half a wide window.
const PANEL_WIDTH: i32 = 380;

/// Build the heavy Full-study analytics (SIF verse-sim, the concept graph, and
/// leitwörter — each a corpus-wide sweep) on first use, caching them in State.
/// A no-op in Simple mode or once built, so launch stays instant.
fn ensure_analytics(state: &Shared) {
    {
        let st = state.borrow();
        if st.analytics_built || !st.mode.is_full() {
            return;
        }
    }
    // Compute under a shared borrow (reads only), then swap in under a mut one.
    // The embedding + morphology artifacts (multi-MB) are loaded here too, so a
    // Simple reader never parses them.
    let (embedding, morph, verse_sim, concept_engine, leitwort) = {
        let st = state.borrow();
        let data = std::path::Path::new(&st.home).join("data");
        let embedding = embed::load_embedding(canon::TOKENIZATION_VERSION, data.join("concept-vectors.vec"));
        let morph = morph::load_morph(canon::TOKENIZATION_VERSION, data.join("morphology.jsonl"));
        let verse_sim = embedding.as_ref().map(|e| embed::VerseSim::build(e, &st.corpus));
        let concept_engine = concept::Concept::build(&st.corpus);
        let leitwort: HashMap<String, burst::Burst> =
            burst::discover_leitworter(&burst::BurstParams::default(), &st.corpus)
                .into_iter()
                .map(|b| (b.strongs.clone(), b))
                .collect();
        (embedding, morph, verse_sim, concept_engine, leitwort)
    };
    let mut st = state.borrow_mut();
    st.embedding = embedding;
    st.morph = morph;
    st.verse_sim = verse_sim;
    st.concept = Some(concept_engine);
    st.leitwort = Some(leitwort);
    st.analytics_built = true;
}

/// Show the study panel with `markup`; open it if it was hidden. On the
/// hidden→visible transition the split is placed so the panel is a fixed-width
/// sidebar regardless of window size (the `Paned` position is absolute from the
/// left, so on a wide window a fixed 700 would leave the panel enormous).
fn show_study(ui: &Ui, markup: &str) {
    ui.study.set_markup(markup);
    if !ui.study_scroll.is_visible() {
        ui.study_scroll.set_visible(true);
        let total = ui.paned.width();
        if total > PANEL_WIDTH + 200 {
            ui.paned.set_position(total - PANEL_WIDTH);
        }
    }
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

fn load_state(cfg: &Config) -> Result<State, String> {
    // Resolve the data home without requiring OVERLAY_HOME: env override, else a
    // working tree (CWD), else next to the executable, else the user data dir.
    let home: String = match home::resolve_home() {
        Some((path, kind)) => {
            eprintln!("pure-study: data home {} ({})", path.display(), kind.label());
            path.to_string_lossy().into_owned()
        }
        None => {
            eprintln!("pure-study: no data home found; falling back to the working directory");
            ".".to_string()
        }
    };
    let data = std::path::Path::new(&home).join("data");
    let corpus = corpus::load_corpus(data.join("kjv.jsonl")).map_err(|e| e.to_string())?;
    let strongs = strongs::load_strongs(data.join("strongs.json")).map_err(|e| e.to_string())?;
    // Notes are optional: a missing file is not an error.
    let notes = notes::load_notes(data.join("kjv-notes.jsonl")).map_err(|e| e.to_string())?;
    // Weaves (cross-references) load from `home/weaves` (+ suggested); bad files
    // are reported but don't fail the reader.
    let (weaves, _weave_errs) = weave::load_weaves(&home);
    let xrefs = build_xrefs(&weaves);
    let links = weave::link_pairs(&weaves);
    let (threads, _thread_errs) = thread::load_threads(&home);
    let (tags, _tag_errs) = tag::load_tags(&home);
    // Personal per-verse notes (Tier 0 #3), keyed by verse.
    let (usernotes, _note_errs) = usernote::load_notes(&home);
    // TSK cross-references (topical study tier) — optional, absent → empty.
    let xref_ix = crossref::load_cross_refs(data.join("cross-references.tsv"));
    // The OT↔NT bridge — etymology (from strongs.json) fused with any external
    // witnesses + trust priors present under the home.
    let bridge = bridge::FusedBridge::build(&strongs, &home);
    // The text-as-witness (small; silent unless graded-qualified).
    let witness = witness::TextWitness::load(data.join("text-witness.json"));
    // Concept embeddings + morphology are Full-study-only and multi-MB to parse,
    // so they load lazily in `ensure_analytics` (not here) to keep launch quick.
    // SIF verse-similarity model, built once over the embedding (heavy, but the
    // embedding is the only prerequisite; skipped when there's no embedding).
    // The heavy analytics (verse-sim, concept graph, leitwörter) are built
    // lazily on first Full-study lookup — not here — so launch is instant.
    let search_ix = SearchIx::build(&corpus);
    let occ_ix = OccurrenceIx::build(&corpus);
    let renderings = Renderings::build(&corpus);
    let family = register_bundled_fonts();
    // Restore last session's panes, or open the default passage on a fresh
    // install; clamp the active index into range.
    let panes: Vec<Pane> = if cfg.panes.is_empty() {
        vec![Pane::new("John", 3)]
    } else {
        cfg.panes.iter().take(MAX_PANES).map(|p| Pane::new(&p.book, p.chapter)).collect()
    };
    let active = cfg.active.min(panes.len().saturating_sub(1));
    // Resolve the colour theme (Tier 0 #5): the user's choice, with `System`
    // following the platform dark preference the style manager reports.
    let sys_dark = adw::StyleManager::default().is_dark();
    let theme_choice = cfg.theme;
    let palette = theme::palette(theme_choice.resolve(sys_dark));
    // Point the panel-markup helpers at the resolved theme (Tier 0 #5).
    set_markup_palette(&palette);
    Ok(State {
        corpus,
        strongs,
        search_ix,
        occ_ix,
        renderings,
        notes,
        xrefs,
        weaves,
        links,
        threads,
        tags,
        usernotes,
        hits: HashSet::new(),
        xref_ix,
        bridge,
        witness,
        embedding: None,
        morph: None,
        verse_sim: None,
        concept: None,
        leitwort: None,
        analytics_built: false,
        home,
        family,
        font_size: cfg.body_size,
        mode: cfg.mode,
        verse_per_line: cfg.verse_per_line,
        palette,
        theme_choice,
        link_other: false,
        panes,
        active,
    })
}

fn build_ui(app: &adw::Application) {
    let (cfg, first_run) = config::load();
    let state = match load_state(&cfg) {
        Ok(s) => Rc::new(RefCell::new(s)),
        Err(e) => {
            present_error(app, &e);
            return;
        }
    };

    // Force the resolved colour scheme so the chrome (header nav, dropdowns,
    // scrollbars, dialogs) matches the reader theme (Tier 0 #5). The reader
    // itself paints from the palette; this keeps libadwaita's chrome in step.
    adw::StyleManager::default().set_color_scheme(if state.borrow().palette.dark {
        adw::ColorScheme::ForceDark
    } else {
        adw::ColorScheme::ForceLight
    });

    // ── header: brand/title + global search (acts on the active pane) ──────────
    let header = adw::HeaderBar::new();
    let title = adw::WindowTitle::new("pure-study", "1769 KJV");
    header.set_title_widget(Some(&title));

    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("search — word, phrase, or reference"));
    search.set_width_chars(28);

    let threads_btn = gtk::Button::with_label("Threads");
    threads_btn.add_css_class("flat");
    let tags_btn = gtk::Button::with_label("Tags");
    tags_btn.add_css_class("flat");
    let weaves_btn = gtk::Button::with_label("Weaves");
    weaves_btn.add_css_class("flat");
    weaves_btn.set_tooltip_text(Some("Browse the weave library"));
    let link_btn = gtk::Button::with_label("＋ link");
    link_btn.add_css_class("flat");
    link_btn.set_tooltip_text(Some("Weave the two pinned words (click a word in each pane; click another word in the same verse to widen the span)"));
    link_btn.set_sensitive(false);

    // The study tools live together so "Simple reader" mode can hide the whole
    // group at once (decision #4), leaving a clean reader + search + lookup.
    // Suggested / Weave map / Constellation, reading mode, verse-per-line, theme,
    // and Help now live in the primary ≡ menu (built below) — the header keeps
    // just the core browse buttons + search + the menu, so nothing is scattered.
    let study_tools = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    study_tools.append(&threads_btn);
    study_tools.append(&tags_btn);
    study_tools.append(&weaves_btn);
    study_tools.append(&link_btn);
    header.pack_start(&study_tools);

    // Primary menu (≡): the clean home for weave views, reading mode, theme, and
    // help. Its GActions ("win.…") are installed on the window further below.
    let menu_btn = gtk::MenuButton::new();
    menu_btn.set_icon_name("open-menu-symbolic");
    menu_btn.set_tooltip_text(Some("Menu — weave views, reading, theme, help"));
    menu_btn.set_menu_model(Some(&build_primary_menu()));
    header.pack_end(&menu_btn);
    header.pack_end(&search);

    // apply_mode + the primary-menu GActions are installed after the window is
    // built (they need the window's "win" action group); see "primary menu
    // actions" below.

    // ── study side panel ─────────────────────────────────────────────────────
    let study = gtk::Label::new(Some(
        "Double-click a word for its Strong’s entry, or search above.",
    ));
    study.set_wrap(true);
    study.set_xalign(0.0);
    study.set_yalign(0.0);
    study.set_selectable(true);
    study.set_use_markup(true);
    study.add_css_class("studypanel"); // a larger base font — the analytics tiers
                                        // lean on <small>/<x-small>, so bump the base
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
    paned.set_vexpand(true);
    // The panel starts hidden; its width is set on first open (see show_study)
    // relative to the actual window width, so it never balloons on a wide
    // display. A resonable default until then.
    paned.set_position(760);

    // ── canon-overview strip (book map, under the panes) ────────────────────────
    let canon_map = gtk::DrawingArea::new();
    canon_map.set_content_height(30);
    canon_map.set_hexpand(true);
    canon_map.set_tooltip_text(Some("Jump anywhere — click a book"));

    let ui = Ui {
        title,
        study: study.clone(),
        study_scroll,
        paned: paned.clone(),
        pane_row,
        link_layer: link_layer.clone(),
        canon_map: canon_map.clone(),
        link_btn: link_btn.clone(),
        pane_uis: Rc::new(RefCell::new(Vec::new())),
        guard: Rc::new(Cell::new(false)),
        css: install_css(&state.borrow().palette),
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
    // Capture the modifier state just before a link activates, so a Shift/Ctrl-
    // click on a `go:` link opens in the other pane (Tier 0 #8). The capture-
    // phase gesture fires ahead of the label's link activation.
    {
        let state = state.clone();
        let click = gtk::GestureClick::new();
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        click.connect_pressed(move |g, _n, _x, _y| {
            let m = g.current_event_state();
            state.borrow_mut().link_other =
                m.contains(gdk::ModifierType::SHIFT_MASK) || m.contains(gdk::ModifierType::CONTROL_MASK);
        });
        study.add_controller(click);
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
        weaves_btn.connect_clicked(move |_| {
            let m = weaves_list_markup(&state.borrow());
            show_study(&ui, &m);
        });
    }
    // Suggested / Weave map / Constellation, reading mode, verse-per-line, theme,
    // and help are all driven by the primary ≡ menu now — their GActions are
    // installed on the window below (see "primary menu actions").

    // ── search box → results in the study panel ─────────────────────────────────
    {
        let state = state.clone();
        let ui = ui.clone();
        search.connect_search_changed(move |entry| {
            let q = entry.text().to_string();
            if q.trim().is_empty() {
                state.borrow_mut().hits.clear();
                redraw_all(&ui);
                hide_study(&ui);
            } else {
                // Band every hit that falls in a visible chapter (Tier 0 #8).
                {
                    let mut st = state.borrow_mut();
                    st.hits.clear();
                    if let Some(SearchAnswer::Hits { hits, .. }) =
                        search::run_search(&st.corpus, &st.notes, &st.search_ix, &q)
                    {
                        for h in hits {
                            st.hits.insert(h.vref);
                        }
                    }
                }
                let markup = search_markup(&state.borrow(), &q);
                show_study(&ui, &markup);
                redraw_all(&ui);
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

    // Save the reading session (open panes + mode + size) on close, so the app
    // reopens where you left it.
    {
        let state = state.clone();
        window.connect_close_request(move |_| {
            persist_config(&state);
            glib::Propagation::Proceed
        });
    }

    // ── primary menu actions (win.*): drive the ≡ menu ──────────────────────────
    // Weave views — Full-study features, so they are disabled in Simple reader.
    let act_suggested = gio::SimpleAction::new("suggested", None);
    {
        let (state, ui) = (state.clone(), ui.clone());
        act_suggested.connect_activate(move |_, _| {
            let m = suggested_list_markup(&state.borrow());
            show_study(&ui, &m);
        });
    }
    window.add_action(&act_suggested);

    let act_weave_map = gio::SimpleAction::new("weave-map", None);
    {
        let (state, ui) = (state.clone(), ui.clone());
        act_weave_map.connect_activate(move |_, _| show_weave_map(&state, &ui));
    }
    window.add_action(&act_weave_map);

    let act_const = gio::SimpleAction::new("constellation", None);
    {
        let (state, ui) = (state.clone(), ui.clone());
        act_const.connect_activate(move |_, _| show_constellation(&state, &ui));
    }
    window.add_action(&act_const);

    // Theme radio (light · dark · night · follow system), persisted.
    let act_theme = gio::SimpleAction::new_stateful(
        "theme",
        Some(glib::VariantTy::STRING),
        &state.borrow().theme_choice.token().to_variant(),
    );
    {
        let (state, ui, act) = (state.clone(), ui.clone(), act_theme.clone());
        act_theme.connect_activate(move |_, param| {
            if let Some(choice) = param.and_then(|p| p.str()).and_then(theme::ThemeChoice::parse) {
                apply_theme_choice(&state, &ui, choice);
                act.set_state(&choice.token().to_variant());
            }
        });
    }
    window.add_action(&act_theme);

    // Verse-per-line checkbox, persisted.
    let act_vpl =
        gio::SimpleAction::new_stateful("verse-per-line", None, &state.borrow().verse_per_line.to_variant());
    {
        let (state, ui) = (state.clone(), ui.clone());
        act_vpl.connect_activate(move |a, _| {
            let now = {
                let mut st = state.borrow_mut();
                st.verse_per_line = !st.verse_per_line;
                st.verse_per_line
            };
            persist_config(&state);
            a.set_state(&now.to_variant());
            redraw_all(&ui);
        });
    }
    window.add_action(&act_vpl);

    // Reading-mode radio (Simple reader ⇄ Full study), persisted.
    let act_mode = gio::SimpleAction::new_stateful(
        "mode",
        Some(glib::VariantTy::STRING),
        &(if state.borrow().mode.is_full() { "full" } else { "simple" }).to_variant(),
    );
    window.add_action(&act_mode);

    // Help.
    let act_guide = gio::SimpleAction::new("guide", None);
    {
        let ui = ui.clone();
        act_guide.connect_activate(move |_, _| show_study(&ui, &blocks_to_markup(&panel::guide_blocks())));
    }
    window.add_action(&act_guide);

    let act_shortcuts = gio::SimpleAction::new("shortcuts", None);
    {
        let (state, ui) = (state.clone(), ui.clone());
        act_shortcuts.connect_activate(move |_, _| show_shortcuts(&state, &ui));
    }
    window.add_action(&act_shortcuts);

    let act_about = gio::SimpleAction::new("about", None);
    {
        let ui = ui.clone();
        act_about.connect_activate(move |_, _| show_study(&ui, &blocks_to_markup(&panel::about_blocks())));
    }
    window.add_action(&act_about);

    // Reflect a study mode across the chrome: show/hide the study tools, gate the
    // Full-study-only weave views, and keep the menu's mode radio in sync.
    let apply_mode: Rc<dyn Fn(StudyMode)> = {
        let study_tools = study_tools.clone();
        let ui = ui.clone();
        let (a_sug, a_map, a_con, a_mode) =
            (act_suggested.clone(), act_weave_map.clone(), act_const.clone(), act_mode.clone());
        Rc::new(move |mode: StudyMode| {
            let full = mode.is_full();
            study_tools.set_visible(full);
            a_sug.set_enabled(full);
            a_map.set_enabled(full);
            a_con.set_enabled(full);
            a_mode.set_state(&(if full { "full" } else { "simple" }).to_variant());
            if !full {
                hide_study(&ui);
            }
        })
    };
    {
        let (state, apply_mode) = (state.clone(), apply_mode.clone());
        act_mode.connect_activate(move |_, param| {
            let mode = match param.and_then(|p| p.str()) {
                Some("full") => StudyMode::Full,
                _ => StudyMode::Simple,
            };
            state.borrow_mut().mode = mode;
            persist_config(&state);
            apply_mode(mode);
        });
    }
    apply_mode(state.borrow().mode);

    install_app_icon();
    rebuild_panes(&state, &ui); // builds the pane columns + first paint
    window.present();

    // Warm the Full-study analytics just after first paint so the first study
    // click doesn't stall building them (Tier 0 #6). GTK's engine state is
    // single-threaded (Rc<RefCell>), so this runs on the main loop right after
    // launch rather than on a background thread (a logged delta vs WinUI, which
    // warms off-thread via pure_engine_warm_indexes).
    if state.borrow().mode.is_full() {
        let state = state.clone();
        glib::timeout_add_local_once(Duration::from_millis(200), move || ensure_analytics(&state));
    }

    // First launch: ask Simple reader vs Full study, then remember the choice.
    if first_run {
        show_mode_chooser(&window, &state, apply_mode.clone());
    }

    // Debug hook: PURE_STUDY_VIEW=constellation|map opens a popup view right
    // after startup (for screenshot-driven development; harmless otherwise).
    if let Ok(v) = std::env::var("PURE_STUDY_VIEW") {
        let (state, ui) = (state.clone(), ui.clone());
        glib::timeout_add_local_once(Duration::from_millis(400), move || match v.as_str() {
            "constellation" => show_constellation(&state, &ui),
            "map" => show_weave_map(&state, &ui),
            _ => {}
        });
    }
}

/// Draw a concept's dispersion strip: the 66 books on the canon axis, each
/// shaded by how densely `code` occurs there. Ported from `ConceptMap`.
fn draw_dispersion(state: &Shared, code: &str, cr: &cairo::Context, w: i32, h: i32) {
    let st = state.borrow();
    let width = w as f64;
    let hf = h as f64;
    let nb = canon::BOOKS.len() as f64;
    cr.set_source_rgb(0.949, 0.933, 0.902);
    let _ = cr.rectangle(0.0, 0.0, width, hf);
    let _ = cr.fill();

    let by_book = st.concept.as_ref().and_then(|ce| ce.stat(code)).map(|s| s.by_book.clone()).unwrap_or_default();
    let max = by_book.values().copied().max().unwrap_or(1) as f64;
    for (i, b) in canon::BOOKS.iter().enumerate() {
        let cnt = by_book.get(b.id).copied().unwrap_or(0) as f64;
        if cnt > 0.0 {
            let x0 = i as f64 / nb * width;
            let x1 = (i as f64 + 1.0) / nb * width;
            cr.set_source_rgba(0.62, 0.49, 0.22, 0.15 + 0.75 * (cnt / max));
            let _ = cr.rectangle(x0, 0.0, x1 - x0, hf);
            let _ = cr.fill();
        }
    }
    // OT/NT seam
    let dx = OT_NT_DIVIDE as f64 / nb * width;
    cr.set_source_rgba(0.4, 0.3, 0.2, 0.5);
    let _ = cr.rectangle(dx - 0.5, 0.0, 1.0, hf);
    let _ = cr.fill();
}

/// Draw a radial concept-neighbourhood: `code` at the centre, its collocation
/// community + embedding neighbours fanned out as labelled spokes. Ported from
/// `ConceptGraph` (static rendering; re-centre is via the word study links).
fn draw_concept_radial(state: &Shared, code: &str, cr: &cairo::Context, w: i32, h: i32) {
    let st = state.borrow();
    let width = w as f64;
    let hf = h as f64;
    cr.set_source_rgb(0.988, 0.976, 0.957);
    let _ = cr.rectangle(0.0, 0.0, width, hf);
    let _ = cr.fill();

    // Neighbours: embedding near (if built) ∪ collocation community, deduped —
    // the shared assembly lives in `concept::radial_spokes` (review item 4), fed
    // the same spokes the non-Rust shells get via `pure_engine_concept_map_json`.
    let near: Vec<String> = st
        .embedding
        .as_ref()
        .map(|emb| emb.nearest_concepts(code, 6).into_iter().map(|(c, _)| c).collect())
        .unwrap_or_default();
    let community = st.concept.as_ref().map(|ce| ce.community(code)).unwrap_or_default();
    let nbrs = concept::radial_spokes(&near, &community, 6); // (code, is_semantic)

    let layout = pangocairo::functions::create_layout(cr);
    let mut fd = pango::FontDescription::new();
    fd.set_family(&st.family);
    fd.set_absolute_size(12.0 * pango::SCALE as f64);
    layout.set_font_description(Some(&fd));

    let (cx, cy) = (width / 2.0, hf / 2.0);
    let radius = (width.min(hf) / 2.0 - 60.0).max(40.0);
    let n = nbrs.len().max(1);
    // English-first node labels: the recognisable gloss on top, lemma beneath.
    let label_of = |c: &str| {
        let en = english_gloss(&st, c);
        let lem = st.strongs.get(c).and_then(|e| e.lemma.clone());
        match (en, lem) {
            (Some(e), Some(l)) => format!("{e}\n{l}"),
            (Some(e), None) => e,
            (None, Some(l)) => l,
            (None, None) => c.to_string(),
        }
    };

    // spokes
    for (k, (c, semantic)) in nbrs.iter().enumerate() {
        let ang = std::f64::consts::TAU * k as f64 / n as f64 - std::f64::consts::FRAC_PI_2;
        let (nx, ny) = (cx + radius * ang.cos(), cy + radius * ang.sin());
        // edge
        if *semantic {
            cr.set_source_rgba(0.62, 0.49, 0.22, 0.5); // gold: distributional
        } else {
            cr.set_source_rgba(0.42, 0.55, 0.40, 0.5); // green: collocation
        }
        cr.set_line_width(1.4);
        cr.move_to(cx, cy);
        cr.line_to(nx, ny);
        let _ = cr.stroke();
        // node label
        let label = label_of(c);
        layout.set_text(&label);
        let (tw, _th) = layout.pixel_size();
        cr.set_source_rgb(0.25, 0.22, 0.17);
        cr.move_to(nx - tw as f64 / 2.0, ny + 6.0);
        pangocairo::functions::show_layout(cr, &layout);
        cr.set_source_rgba(0.62, 0.49, 0.22, 0.9);
        cr.arc(nx, ny, 3.0, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
    }
    // centre node
    fd.set_absolute_size(15.0 * pango::SCALE as f64);
    layout.set_font_description(Some(&fd));
    layout.set_text(&label_of(code));
    let (tw, _) = layout.pixel_size();
    cr.set_source_rgb(0.62, 0.49, 0.22);
    cr.arc(cx, cy, 5.0, 0.0, std::f64::consts::TAU);
    let _ = cr.fill();
    cr.set_source_rgb(0.20, 0.16, 0.10);
    cr.move_to(cx - tw as f64 / 2.0, cy - 26.0);
    pangocairo::functions::show_layout(cr, &layout);
}

/// Open the concept map for `code`: a radial neighbourhood over a dispersion
/// strip. Esc closes.
fn show_concept_map(state: &Shared, ui: &Ui, code: &str) {
    ensure_analytics(state);
    let win = gtk::Window::builder()
        .title(&format!("Concept map — {code}"))
        .default_width(720)
        .default_height(560)
        .build();
    if let Some(p) = window_of(ui) {
        win.set_transient_for(Some(&p));
    }
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let radial = gtk::DrawingArea::new();
    radial.set_vexpand(true);
    radial.set_hexpand(true);
    {
        let (state, code) = (state.clone(), code.to_string());
        radial.set_draw_func(move |_a, cr, w, h| draw_concept_radial(&state, &code, cr, w, h));
    }
    let strip = gtk::DrawingArea::new();
    strip.set_content_height(40);
    strip.set_hexpand(true);
    strip.set_tooltip_text(Some("Dispersion: where across the 66 books this concept occurs"));
    {
        let (state, code) = (state.clone(), code.to_string());
        strip.set_draw_func(move |_a, cr, w, h| draw_dispersion(&state, &code, cr, w, h));
    }
    vbox.append(&radial);
    vbox.append(&strip);
    {
        let win2 = win.clone();
        let key = gtk::EventControllerKey::new();
        key.connect_key_pressed(move |_c, k, _kc, _m| {
            if k == gdk::Key::Escape {
                win2.close();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        win.add_controller(key);
    }
    win.set_child(Some(&vbox));
    win.present();
}

/// Open the book-to-book weave chord map in a popup: click a book to jump the
/// active pane there; Esc closes.
fn show_weave_map(state: &Shared, ui: &Ui) {
    let win = gtk::Window::builder().title("Weave map").default_width(1000).default_height(360).build();
    if let Some(p) = window_of(ui) {
        win.set_transient_for(Some(&p));
    }
    let area = gtk::DrawingArea::new();
    area.set_hexpand(true);
    area.set_vexpand(true);
    {
        let state = state.clone();
        area.set_draw_func(move |_a, cr, w, h| draw_chord_map(&state, cr, w, h));
    }
    {
        let state = state.clone();
        let ui = ui.clone();
        let win = win.clone();
        let area2 = area.clone();
        let click = gtk::GestureClick::new();
        click.connect_pressed(move |_g, _n, x, _y| {
            let w = area2.width();
            if w <= 0 {
                return;
            }
            let idx = ((x / w as f64).clamp(0.0, 0.999) * canon::BOOKS.len() as f64) as usize;
            if let Some(b) = canon::BOOKS.get(idx) {
                let active = state.borrow().active;
                navigate_pane(&state, &ui, active, b.id, 1, None);
            }
            win.close();
        });
        area.add_controller(click);
    }
    {
        let win2 = win.clone();
        let key = gtk::EventControllerKey::new();
        key.connect_key_pressed(move |_c, k, _kc, _m| {
            if k == gdk::Key::Escape {
                win2.close();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        win.add_controller(key);
    }
    win.set_child(Some(&area));
    win.present();
}

// ── constellation ───────────────────────────────────────────────────────────
// Ported from overlay `Constellation.hs` + its UI.hs wiring: a scoped overview
// of the weave library, one weave per labelled lane (largest first), nodes on
// the canon book backbone, links as gentle curves. Pinned lanes stay put while
// paging cycles the free lanes past them; click a node to jump, an edge to open
// the weave, the gutter marker to pin.

// The lane count (18) lives in the shared view-model (`CONSTELLATION_LANES`,
// echoed as `lane_capacity`); these are the shell's paint-only geometry.
const CONST_TOP_PAD: f64 = 18.0;
const CONST_GUTTER: f64 = 150.0;
/// Left edge of the plot: past the gutter plus a margin so a frac-0 node
/// (Genesis 1:1) sits clear of the pin gutter and can't be swallowed by a pin
/// click.
const CONST_PLOT_LEFT: f64 = CONST_GUTTER + 12.0;

/// Transient view state: the current page, the pinned weave **indices**, and
/// the hovered `(lane, node)` on the current page. Dropped when the window
/// closes (overlay kept these transient too — `amConstPage`/`amConstPins`). The
/// layout itself now lives in `weave::constellation`, shared with the non-Rust
/// shells via `pure_engine_constellation_json`; this struct holds only the
/// interaction state fed into it.
struct ConstState {
    page: usize,
    pins: Vec<usize>,
    hover: Option<(usize, usize)>,
}

fn const_x(w: f64, frac: f64) -> f64 {
    CONST_PLOT_LEFT + frac.clamp(0.0, 1.0) * (w - CONST_PLOT_LEFT)
}

/// The lane band height: a fixed capacity with a small bottom margin so the
/// last lane never clips — the same mapping the non-Rust shells use, so a node
/// lands at the same spot in every shell.
fn const_lane_h(h: f64, lane_capacity: usize) -> f64 {
    (h - CONST_TOP_PAD - 10.0) / lane_capacity.max(1) as f64
}

/// A node/edge endpoint's pixel position from its fractions: `x_frac` across the
/// plot, `lane_frac` within lane `lane`'s band.
fn const_node_xy(
    w: f64,
    h: f64,
    lane: usize,
    x_frac: f32,
    lane_frac: f32,
    lane_capacity: usize,
) -> (f64, f64) {
    let y = CONST_TOP_PAD + (lane as f64 + lane_frac as f64) * const_lane_h(h, lane_capacity);
    (const_x(w, x_frac as f64), y)
}

/// The node nearest `p` within its radius + 4px slop, as `(lane, node)`,
/// tie-broken by distance — over the shared view-model.
fn const_hit_node(
    model: &weave::Constellation,
    w: f64,
    h: f64,
    p: (f64, f64),
) -> Option<(usize, usize)> {
    let cap = model.lane_capacity;
    let mut best: Option<(usize, usize)> = None;
    let mut best_d = f64::INFINITY;
    for (lane, l) in model.lanes.iter().enumerate() {
        for (ni, n) in l.nodes.iter().enumerate() {
            let (x, y) = const_node_xy(w, h, lane, n.x, n.lane_frac, cap);
            let half = 1.4 + 2.4 * n.size as f64;
            let d = ((p.0 - x).powi(2) + (p.1 - y).powi(2)).sqrt();
            if d <= half + 4.0 && d < best_d {
                best_d = d;
                best = Some((lane, ni));
            }
        }
    }
    best
}

/// The weave index of the lane whose drawn edge passes within 5px of `p` (an
/// edge click opens that weave's card).
fn const_hit_edge(model: &weave::Constellation, w: f64, h: f64, p: (f64, f64)) -> Option<usize> {
    let cap = model.lane_capacity;
    for (lane, l) in model.lanes.iter().enumerate() {
        for e in &l.edges {
            let pa = const_node_xy(w, h, lane, e.a_x, e.a_lane_frac, cap);
            let pb = const_node_xy(w, h, lane, e.b_x, e.b_lane_frac, cap);
            if curve_dist(p, pa, pb) <= 5.0 {
                return Some(l.weave_index);
            }
        }
    }
    None
}

/// Distinct lane colours (cycled), darkened from the overlay's palette to keep
/// contrast on the warm-paper background.
fn const_rgb(lane: usize) -> (f64, f64, f64) {
    let (r, g, b) = match lane % 7 {
        0 => (210, 180, 110),
        1 => (127, 180, 230),
        2 => (143, 184, 138),
        3 => (217, 140, 140),
        4 => (184, 156, 214),
        5 => (150, 194, 190),
        _ => (214, 170, 128),
    };
    (r as f64 / 255.0 * 0.72, g as f64 / 255.0 * 0.72, b as f64 / 255.0 * 0.72)
}

/// Sampled points of the connector cubic (18 segments, like overlay
/// `curveSamples`) so edge hit-testing measures the curve the eye sees.
fn curve_samples(x1: f64, y1: f64, x2: f64, y2: f64) -> Vec<(f64, f64)> {
    let dx = x2 - x1;
    let (c1x, c1y, c2x, c2y) = (x1 + dx * 0.4, y1, x2 - dx * 0.4, y2);
    (0..=18)
        .map(|i| {
            let t = i as f64 / 18.0;
            let u = 1.0 - t;
            let x = u * u * u * x1 + 3.0 * u * u * t * c1x + 3.0 * u * t * t * c2x + t * t * t * x2;
            let y = u * u * u * y1 + 3.0 * u * u * t * c1y + 3.0 * u * t * t * c2y + t * t * t * y2;
            (x, y)
        })
        .collect()
}

/// Distance from `p` to segment `a`–`b`.
fn seg_dist(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let len2 = vx * vx + vy * vy;
    let t = if len2 <= 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / len2).clamp(0.0, 1.0)
    };
    let (cx, cy) = (a.0 + t * vx, a.1 + t * vy);
    ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()
}

/// Distance from `p` to the drawn connector between two endpoints.
fn curve_dist(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let pts = curve_samples(a.0, a.1, b.0, b.1);
    pts.windows(2)
        .map(|w| seg_dist(p, w[0], w[1]))
        .fold(f64::INFINITY, f64::min)
}

fn draw_constellation(
    state: &Shared,
    cs: &Rc<RefCell<ConstState>>,
    cr: &cairo::Context,
    w: i32,
    h: i32,
) {
    let st = state.borrow();
    let cs = cs.borrow();
    let (wf, hf) = (w as f64, h as f64);
    // The whole layout — usable filter, largest-first order, per-verse degree,
    // jitter, lane assignment, paging, pins — is the shared core view-model
    // (`weave::constellation`), the same one the non-Rust shells get as JSON.
    let model = weave::constellation(&st.weaves, &st.corpus, cs.page, &cs.pins);
    let cap = model.lane_capacity;
    let lane_h = const_lane_h(hf, cap);

    // warm-paper backdrop, like the reader
    cr.set_source_rgb(0.988, 0.976, 0.957);
    let _ = cr.rectangle(0.0, 0.0, wf, hf);
    let _ = cr.fill();

    let layout = pangocairo::functions::create_layout(cr);
    let mut fd = pango::FontDescription::new();
    fd.set_family(&st.family);

    // alternating lane bands over the full capacity
    for i in 0..cap {
        if i % 2 == 0 {
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.03);
            let _ = cr.rectangle(0.0, CONST_TOP_PAD + i as f64 * lane_h, wf, lane_h);
            let _ = cr.fill();
        }
    }

    // per lane: pin marker + weave name
    for (lane, l) in model.lanes.iter().enumerate() {
        let mid = CONST_TOP_PAD + lane as f64 * lane_h + lane_h / 2.0;
        // pin marker: filled gold when pinned, hollow otherwise
        if l.pinned {
            cr.set_source_rgb(0.62, 0.49, 0.22);
            let _ = cr.rectangle(6.0, mid - 4.0, 8.0, 8.0);
            let _ = cr.fill();
        } else {
            cr.set_source_rgba(0.45, 0.45, 0.45, 0.55);
            cr.set_line_width(1.0);
            let _ = cr.rectangle(6.5, mid - 3.5, 7.0, 7.0);
            let _ = cr.stroke();
        }
        let name: String = l.name.chars().take(22).collect();
        fd.set_absolute_size(10.5 * pango::SCALE as f64);
        layout.set_font_description(Some(&fd));
        layout.set_text(&name);
        if l.pinned {
            cr.set_source_rgb(0.55, 0.42, 0.15);
        } else {
            cr.set_source_rgb(0.35, 0.33, 0.30);
        }
        cr.move_to(20.0, mid - 7.0);
        pangocairo::functions::show_layout(cr, &layout);
    }

    // canon section dividers + ruler labels
    fd.set_absolute_size(10.0 * pango::SCALE as f64);
    layout.set_font_description(Some(&fd));
    let nb = canon::BOOKS.len() as f64;
    for (lbl, lo, _hi) in CANON_SEGMENTS.iter() {
        let x0 = const_x(wf, *lo as f64 / nb);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.06);
        let _ = cr.rectangle(x0, CONST_TOP_PAD, 1.0, hf - CONST_TOP_PAD);
        let _ = cr.fill();
        cr.set_source_rgb(0.42, 0.40, 0.36);
        layout.set_text(lbl);
        cr.move_to(x0 + 3.0, 2.0);
        pangocairo::functions::show_layout(cr, &layout);
    }
    // the OT/NT seam
    let dxx = const_x(wf, OT_NT_DIVIDE as f64 / nb);
    cr.set_source_rgba(0.62, 0.49, 0.22, 0.6);
    let _ = cr.rectangle(dxx - 0.5, 0.0, 1.0, hf);
    let _ = cr.fill();

    // edges (faint, per-lane colour) under the nodes
    for (lane, l) in model.lanes.iter().enumerate() {
        let (r, g, b) = const_rgb(lane);
        cr.set_source_rgba(r, g, b, 0.5);
        cr.set_line_width(1.0);
        for e in &l.edges {
            let (x1, y1) = const_node_xy(wf, hf, lane, e.a_x, e.a_lane_frac, cap);
            let (x2, y2) = const_node_xy(wf, hf, lane, e.b_x, e.b_lane_frac, cap);
            let ddx = x2 - x1;
            cr.move_to(x1, y1);
            cr.curve_to(x1 + ddx * 0.4, y1, x2 - ddx * 0.4, y2, x2, y2);
            let _ = cr.stroke();
        }
    }

    // nodes, sized by witness degree
    for (lane, l) in model.lanes.iter().enumerate() {
        let (cr_, cg, cb) = const_rgb(lane);
        cr.set_source_rgb(cr_, cg, cb);
        for n in &l.nodes {
            let (x, y) = const_node_xy(wf, hf, lane, n.x, n.lane_frac, cap);
            let rr = 1.4 + 2.4 * n.size as f64;
            let _ = cr.rectangle(x - rr, y - rr, 2.0 * rr, 2.0 * rr);
            let _ = cr.fill();
        }
    }

    // hover tooltip: verse · weave
    if let Some((lane, node)) = cs.hover {
        if let Some(n) = model.lanes.get(lane).and_then(|l| l.nodes.get(node)) {
            let l = &model.lanes[lane];
            let (x, y) = const_node_xy(wf, hf, lane, n.x, n.lane_frac, cap);
            let txt = format!("{} · {}", n.display, l.name);
            fd.set_absolute_size(11.0 * pango::SCALE as f64);
            layout.set_font_description(Some(&fd));
            layout.set_text(&txt);
            let (tw, th) = layout.pixel_size();
            let (bw, bh) = (tw as f64 + 12.0, th as f64 + 6.0);
            let bx = (x + 8.0).min(wf - bw - 2.0).max(2.0);
            let by = (y - bh - 6.0).max(2.0);
            cr.set_source_rgba(0.09, 0.10, 0.11, 0.96);
            let _ = cr.rectangle(bx, by, bw, bh);
            let _ = cr.fill();
            cr.set_source_rgb(0.92, 0.90, 0.87);
            cr.move_to(bx + 6.0, by + 3.0);
            pangocairo::functions::show_layout(cr, &layout);
        }
    }
}

/// Open the constellation: one page of weaves as labelled lanes over the canon
/// backbone. ‹/› (or Left/Right) page the free lanes past the pinned ones;
/// click a node to jump the active pane there, an edge to open its weave card,
/// the gutter marker to pin/unpin. Esc or the close button dismisses it.
fn show_constellation(state: &Shared, ui: &Ui) {
    let win = gtk::Window::builder()
        .title("Constellation")
        .default_width(1200)
        .default_height(640)
        .build();
    if let Some(p) = window_of(ui) {
        win.set_transient_for(Some(&p));
    }
    let cs = Rc::new(RefCell::new(ConstState { page: 0, pins: Vec::new(), hover: None }));

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    controls.set_margin_top(6);
    controls.set_margin_start(8);
    controls.set_margin_end(8);
    let prev = gtk::Button::with_label("‹ prev");
    prev.add_css_class("flat");
    let next = gtk::Button::with_label("next ›");
    next.add_css_class("flat");
    let caption = gtk::Label::new(None);
    caption.add_css_class("dim-label");
    controls.append(&prev);
    controls.append(&next);
    controls.append(&caption);

    let area = gtk::DrawingArea::new();
    area.set_hexpand(true);
    area.set_vexpand(true);
    vbox.append(&controls);
    vbox.append(&area);

    // One refresher shared by every handler: clamp the page, redraw, recaption —
    // all off the shared view-model.
    let refresh: Rc<dyn Fn()> = {
        let (state, cs, area, caption) = (state.clone(), cs.clone(), area.clone(), caption.clone());
        Rc::new(move || {
            let model = {
                let st = state.borrow();
                let c = cs.borrow();
                weave::constellation(&st.weaves, &st.corpus, c.page, &c.pins)
            };
            cs.borrow_mut().page = model.page;
            caption.set_text(&model.caption);
            area.queue_draw();
        })
    };
    refresh();

    {
        let (state, cs) = (state.clone(), cs.clone());
        area.set_draw_func(move |_a, cr, w, h| draw_constellation(&state, &cs, cr, w, h));
    }

    // hover → tooltip node
    {
        let (state, cs, area2) = (state.clone(), cs.clone(), area.clone());
        let (cs_leave, area_leave) = (cs.clone(), area.clone());
        let motion = gtk::EventControllerMotion::new();
        motion.connect_motion(move |_c, x, y| {
            let model = {
                let st = state.borrow();
                let c = cs.borrow();
                weave::constellation(&st.weaves, &st.corpus, c.page, &c.pins)
            };
            let hover =
                const_hit_node(&model, area2.width() as f64, area2.height() as f64, (x, y));
            if cs.borrow().hover != hover {
                cs.borrow_mut().hover = hover;
                area2.queue_draw();
            }
        });
        motion.connect_leave(move |_| {
            if cs_leave.borrow().hover.is_some() {
                cs_leave.borrow_mut().hover = None;
                area_leave.queue_draw();
            }
        });
        area.add_controller(motion);
    }

    // click: node → jump · edge → weave card · gutter → pin
    {
        let (state, ui, cs, win2, area2, refresh) =
            (state.clone(), ui.clone(), cs.clone(), win.clone(), area.clone(), refresh.clone());
        let click = gtk::GestureClick::new();
        click.connect_pressed(move |_g, _n, x, y| {
            let (wf, hf) = (area2.width() as f64, area2.height() as f64);
            // `weave::constellation` returns owned data, so the state borrow is
            // released here — no conflict with the mutating actions below.
            let model = {
                let st = state.borrow();
                let c = cs.borrow();
                weave::constellation(&st.weaves, &st.corpus, c.page, &c.pins)
            };
            // A node/edge always wins over the pin gutter, so a Genesis node on
            // the plot edge navigates rather than pinning.
            if let Some((lane, node)) = const_hit_node(&model, wf, hf, (x, y)) {
                let n = &model.lanes[lane].nodes[node];
                let active = state.borrow().active;
                navigate_pane(&state, &ui, active, &n.book, n.chapter, Some(n.verse));
            } else if let Some(wi) = const_hit_edge(&model, wf, hf, (x, y)) {
                let m = weave_markup(&state.borrow(), wi);
                show_study(&ui, &m);
                win2.close();
            } else if x < CONST_GUTTER {
                // pin gutter: left of the plot, on a lane
                let lane_h = const_lane_h(hf, model.lane_capacity);
                let lane = ((y - CONST_TOP_PAD) / lane_h).floor();
                if lane >= 0.0 && (lane as usize) < model.lanes.len() {
                    let wi = model.lanes[lane as usize].weave_index;
                    let mut c = cs.borrow_mut();
                    match c.pins.iter().position(|p| *p == wi) {
                        Some(i) => {
                            c.pins.remove(i);
                        }
                        None => c.pins.push(wi),
                    }
                    drop(c);
                    refresh();
                }
            }
        });
        area.add_controller(click);
    }

    // paging: buttons + Left/Right keys; Esc closes
    {
        let (cs, refresh) = (cs.clone(), refresh.clone());
        prev.connect_clicked(move |_| {
            let p = cs.borrow().page;
            cs.borrow_mut().page = p.saturating_sub(1);
            refresh();
        });
    }
    {
        let (cs, refresh) = (cs.clone(), refresh.clone());
        next.connect_clicked(move |_| {
            cs.borrow_mut().page += 1; // refresh clamps to max_page
            refresh();
        });
    }
    {
        let (cs, refresh, win2) = (cs.clone(), refresh.clone(), win.clone());
        let key = gtk::EventControllerKey::new();
        key.connect_key_pressed(move |_c, k, _kc, _m| match k {
            gdk::Key::Escape => {
                win2.close();
                glib::Propagation::Stop
            }
            gdk::Key::Left => {
                let p = cs.borrow().page;
                cs.borrow_mut().page = p.saturating_sub(1);
                refresh();
                glib::Propagation::Stop
            }
            gdk::Key::Right => {
                cs.borrow_mut().page += 1;
                refresh();
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        });
        win.add_controller(key);
    }

    win.set_child(Some(&vbox));
    win.present();
}

/// Persist the current mode + body size + reading session (open panes) to the
/// platform config (best-effort; the reader keeps working if it can't write).
fn persist_config(state: &Shared) {
    let cfg = {
        let st = state.borrow();
        Config {
            mode: st.mode,
            body_size: st.font_size,
            panes: st
                .panes
                .iter()
                .map(|p| config::PaneRef { book: p.book.clone(), chapter: p.chapter })
                .collect(),
            active: st.active,
            verse_per_line: st.verse_per_line,
            theme: st.theme_choice,
        }
    };
    let _ = config::save(&cfg);
}

/// The guided first-run chooser: a modal offering Simple reader vs Full study,
/// each with a one-line description. The pick is applied to the header and
/// saved; closing without choosing keeps the default (Simple).
fn show_mode_chooser(parent: &adw::ApplicationWindow, state: &Shared, apply_mode: Rc<dyn Fn(StudyMode)>) {
    let win = gtk::Window::builder()
        .title("Welcome to pure-study")
        .modal(true)
        .default_width(460)
        .transient_for(parent)
        .build();

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 12);
    vbox.set_margin_top(20);
    vbox.set_margin_bottom(20);
    vbox.set_margin_start(20);
    vbox.set_margin_end(20);

    let intro = gtk::Label::new(None);
    intro.set_use_markup(true);
    intro.set_wrap(true);
    intro.set_xalign(0.0);
    intro.set_markup(
        "<b>How would you like to read?</b>\n\nYou can switch anytime from the toolbar.",
    );
    vbox.append(&intro);

    let choose = |title: &str, blurb: &str| {
        let b = gtk::Button::new();
        b.add_css_class("card");
        let inner = gtk::Box::new(gtk::Orientation::Vertical, 3);
        inner.set_margin_top(8);
        inner.set_margin_bottom(8);
        inner.set_margin_start(10);
        inner.set_margin_end(10);
        let t = gtk::Label::new(None);
        t.set_use_markup(true);
        t.set_xalign(0.0);
        t.set_markup(&format!("<b>{title}</b>"));
        let d = gtk::Label::new(Some(blurb));
        d.set_wrap(true);
        d.set_xalign(0.0);
        d.add_css_class("dim-label");
        inner.append(&t);
        inner.append(&d);
        b.set_child(Some(&inner));
        b
    };

    let simple = choose(
        "Simple reader",
        "Just the text: chapters, search, and a double-click for Strong's. Nothing else in the way.",
    );
    let full = choose(
        "Full study",
        "Everything: threads, tags, weave cross-references and authoring, and the review queue.",
    );
    vbox.append(&simple);
    vbox.append(&full);
    win.set_child(Some(&vbox));

    let pick = {
        let state = state.clone();
        let win = win.clone();
        let apply_mode = apply_mode.clone();
        move |mode: StudyMode| {
            {
                state.borrow_mut().mode = mode;
            }
            persist_config(&state);
            apply_mode(mode);
            win.close();
        }
    };
    {
        let pick = pick.clone();
        simple.connect_clicked(move |_| pick(StudyMode::Simple));
    }
    {
        let pick = pick.clone();
        full.connect_clicked(move |_| pick(StudyMode::Full));
    }

    win.present();
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
        click.connect_pressed(move |g, n_press, x, y| {
            set_active(&state, &ui, i);
            area2.grab_focus();
            let ctrl = g.current_event_state().contains(gdk::ModifierType::CONTROL_MASK);
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
            if n_press == 2 || ctrl {
                // Ctrl+click or double-click → Strong's study (double-click is the
                // discoverable alt for the modifier). Build the Full-study
                // analytics on first use (kept off the launch path).
                ensure_analytics(&state);
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

    // ── drag: paint a word-precise highlight across the pointer (Tier 0 #4) ─────
    // The press already pinned the start word (above); a real drag past the
    // threshold supersedes that pin and lays down a cross-verse highlight.
    {
        let state = state.clone();
        let ui = ui.clone();
        let area2 = area.clone();
        let drag = gtk::GestureDrag::new();
        drag.set_button(gdk::BUTTON_PRIMARY);
        // The start (anchor) hit; the live end lives in Pane::hl_drag, set only
        // once the drag passes the threshold so a plain click never previews.
        let anchor: Rc<RefCell<Option<(VRef, u32)>>> = Rc::new(RefCell::new(None));
        {
            let (state, anchor) = (state.clone(), anchor.clone());
            drag.connect_drag_begin(move |_g, x, y| {
                let hit = {
                    let st = state.borrow();
                    st.panes.get(i).and_then(|p| {
                        p.dl.as_ref().and_then(|dl| dl.hit_test(x as f32 - p.margin_x, y as f32 - MARGIN))
                    })
                };
                *anchor.borrow_mut() = hit.map(|h| (h.verse, h.token_index));
            });
        }
        {
            let (state, area2, anchor) = (state.clone(), area2.clone(), anchor.clone());
            drag.connect_drag_update(move |g, ox, oy| {
                if ox.hypot(oy) < HL_DRAG_THRESHOLD {
                    return;
                }
                let Some((av, at)) = anchor.borrow().clone() else { return };
                let Some((sx, sy)) = g.start_point() else { return };
                let (x, y) = (sx + ox, sy + oy);
                let end = {
                    let st = state.borrow();
                    st.panes.get(i).and_then(|p| {
                        p.dl.as_ref().and_then(|dl| dl.hit_test(x as f32 - p.margin_x, y as f32 - MARGIN))
                    })
                };
                if let Some(h) = end {
                    if let Some(p) = state.borrow_mut().panes.get_mut(i) {
                        p.hl_drag = Some((av, at, h.verse, h.token_index));
                    }
                    area2.queue_draw();
                }
            });
        }
        {
            let (state, ui, anchor) = (state.clone(), ui.clone(), anchor.clone());
            drag.connect_drag_end(move |_g, _ox, _oy| {
                anchor.borrow_mut().take();
                // hl_drag is only set past the threshold, so a plain click leaves
                // it None here and this is a no-op (the click already pinned).
                let sel = state.borrow_mut().panes.get_mut(i).and_then(|p| p.hl_drag.take());
                let Some((sr, stok, er, etok)) = sel else { return };
                if let Some(p) = state.borrow_mut().panes.get_mut(i) {
                    p.pin = None; // the drag supersedes the start-word pin
                }
                let (tone, hex) = theme::HIGHLIGHT_TONES[0];
                highlight_range(&state, &ui, &sr, stok, &er, etok, tone, hex);
            });
        }
        area.add_controller(drag);
    }

    // ── right-click → the verse context menu (Tier 0 #1) ────────────────────────
    {
        let state = state.clone();
        let ui = ui.clone();
        let area2 = area.clone();
        let click = gtk::GestureClick::new();
        click.set_button(gdk::BUTTON_SECONDARY);
        click.connect_pressed(move |_g, _n, x, y| {
            set_active(&state, &ui, i);
            show_context_menu(&state, &ui, i, &area2, x, y);
        });
        area.add_controller(click);
    }

    // ── mouse buttons 4/5 (back/forward) walk the history (Tier 0 #2) ───────────
    {
        let state = state.clone();
        let ui = ui.clone();
        let click = gtk::GestureClick::new();
        click.set_button(0); // any button — we only act on 8 (back) / 9 (forward)
        click.connect_pressed(move |g, _n, _x, _y| match g.current_button() {
            8 => pane_history(&state, &ui, i, -1),
            9 => pane_history(&state, &ui, i, 1),
            _ => {}
        });
        area.add_controller(click);
    }

    // ── wheel: Ctrl zooms every pane · Shift locks the panes together ────────────
    {
        let state = state.clone();
        let ui = ui.clone();
        // BOTH_AXES: with Shift held the compositor often re-routes the wheel to
        // the horizontal axis, so we must read whichever axis carries the delta —
        // otherwise Shift+scroll looks dead (the reported bug).
        let sc = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
        sc.connect_scroll(move |c, dx, dy| {
            let mods = c.current_event_state();
            let d = if dy.abs() >= dx.abs() { dy } else { dx };
            if mods.contains(gdk::ModifierType::CONTROL_MASK) {
                if d.abs() < 0.01 {
                    return glib::Propagation::Stop; // ignore jitter while holding Ctrl
                }
                zoom(&state, &ui, if d < 0.0 { 1.0 } else { -1.0 });
                glib::Propagation::Stop
            } else if mods.contains(gdk::ModifierType::SHIFT_MASK) {
                let px = d * state.borrow().font_size * 3.0;
                scroll_all(&ui, px);
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed // plain wheel: GTK scrolls the pane under the pointer
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
            let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
            let alt = mods.contains(gdk::ModifierType::ALT_MASK);
            let stop = glib::Propagation::Stop;
            // Up/Down move a few lines, PageUp/Down/Space nearly a page. Holding
            // Shift locks every pane together (parallel reading); otherwise just
            // this pane. Home/End/Left/Right always act on this pane alone.
            let line = state.borrow().font_size * 3.0;
            let page = (vadj2.page_size() * 0.85).max(line);
            let vscroll = |px: f64| {
                if shift {
                    scroll_all(&ui, px);
                } else {
                    scroll_by_px(&vadj2, px);
                }
            };
            match key {
                gdk::Key::Up => { vscroll(-line); stop }
                gdk::Key::Down => { vscroll(line); stop }
                gdk::Key::Page_Up => { vscroll(-page); stop }
                gdk::Key::Page_Down | gdk::Key::space => { vscroll(page); stop }
                gdk::Key::Home => { vadj2.set_value(vadj2.lower()); stop }
                gdk::Key::End => {
                    vadj2.set_value((vadj2.upper() - vadj2.page_size()).max(vadj2.lower()));
                    stop
                }
                gdk::Key::Left if alt => { pane_history(&state, &ui, i, -1); stop }
                gdk::Key::Right if alt => { pane_history(&state, &ui, i, 1); stop }
                gdk::Key::Right | gdk::Key::bracketright => { step_pane(&state, &ui, i, 1); stop }
                gdk::Key::Left | gdk::Key::bracketleft => { step_pane(&state, &ui, i, -1); stop }
                gdk::Key::_0 | gdk::Key::KP_0 if ctrl => { zoom(&state, &ui, 0.0); stop }
                gdk::Key::plus | gdk::Key::equal if ctrl => { zoom(&state, &ui, 1.0); stop }
                gdk::Key::minus if ctrl => { zoom(&state, &ui, -1.0); stop }
                gdk::Key::F1 | gdk::Key::question => { show_shortcuts(&state, &ui); stop }
                gdk::Key::Escape => { hide_study(&ui); stop }
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
        // Record the destination in the reading history unless this navigation
        // *is* a history move (Tier 0 #2); forward entries past the cursor drop.
        if !p.in_history_nav {
            if p.hist_idx >= 0 && (p.hist_idx as usize) + 1 < p.history.len() {
                p.history.truncate(p.hist_idx as usize + 1);
            }
            let cur = (p.book.clone(), p.chapter);
            if p.history.last() != Some(&cur) {
                p.history.push(cur);
                p.hist_idx = p.history.len() as isize - 1;
            }
        }
        p.scroll_to = verse;
        p.highlight = verse;
        // Drop the stale layout so the verse scroll can't act on the old
        // chapter — the pending scroll waits for the new chapter's paint.
        p.dl = None;
        p.last_h = 0;
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
            // Poll briefly until the new layout is painted and its scroll
            // extent is valid, then scroll once. Fires as soon as it's ready
            // (typically the very next frame) instead of betting on a fixed
            // delay, and gives up after ~1s so a missing verse can't spin.
            let state = state.clone();
            let ui = ui.clone();
            let mut tries = 0u32;
            glib::timeout_add_local(Duration::from_millis(8), move || {
                tries += 1;
                if try_scroll_pane(&state, &ui, i) || tries > 120 {
                    glib::ControlFlow::Break
                } else {
                    glib::ControlFlow::Continue
                }
            });
        }
    }
}

/// Step pane `i`'s chapter, rolling across book boundaries (Tier 0 #8): past the
/// last chapter enters the next book, before chapter 1 enters the previous.
fn step_pane(state: &Shared, ui: &Ui, i: usize, delta: i32) {
    let target = {
        let st = state.borrow();
        let p = &st.panes[i];
        let count = st.corpus.chapter_count(&p.book) as i32;
        let ch = p.chapter as i32 + delta;
        if ch < 1 {
            canon::adjacent_book(&p.book, -1)
                .map(|b| (b.to_string(), st.corpus.chapter_count(b).max(1)))
        } else if ch > count {
            canon::adjacent_book(&p.book, 1).map(|b| (b.to_string(), 1u16))
        } else {
            Some((p.book.clone(), ch as u16))
        }
    };
    if let Some((book, ch)) = target {
        navigate_pane(state, ui, i, &book, ch, None);
    }
}

/// Walk pane `i`'s reading history by `delta` (−1 back, +1 forward). Navigates
/// without pushing a new history entry.
fn pane_history(state: &Shared, ui: &Ui, i: usize, delta: isize) {
    let target = {
        let mut st = state.borrow_mut();
        let Some(p) = st.panes.get_mut(i) else { return };
        let new_idx = p.hist_idx + delta;
        if new_idx < 0 || new_idx >= p.history.len() as isize {
            return;
        }
        p.hist_idx = new_idx;
        p.in_history_nav = true;
        p.history[new_idx as usize].clone()
    };
    navigate_pane(state, ui, i, &target.0, target.1, None);
    if let Some(p) = state.borrow_mut().panes.get_mut(i) {
        p.in_history_nav = false;
    }
}

/// Try to scroll pane `i` so its pending target verse sits near the top.
/// Returns `true` when the work is done — either the scroll succeeded, there was
/// nothing pending, or the pane is gone — and `false` while it must keep waiting
/// for the new chapter's layout (and its scroll extent) to be ready.
fn try_scroll_pane(state: &Shared, ui: &Ui, i: usize) -> bool {
    let (y, want_h) = {
        let st = state.borrow();
        let Some(p) = st.panes.get(i) else { return true };
        let Some(target) = p.scroll_to else { return true };
        // Wait for this chapter's paint (`dl` was cleared on navigate).
        let Some(dl) = p.dl.as_ref() else { return false };
        match dl
            .items
            .iter()
            .find(|it| matches!(it.kind, ItemKind::VerseNumber(n) if n == target))
        {
            Some(it) => ((MARGIN + it.y) as f64, p.last_h as f64),
            None => return false,
        }
    };
    let Some(pu) = ui.pane_uis.borrow().get(i).map(|pu| pu.vadj.clone()) else {
        return true;
    };
    // The scroll extent must reflect the new content height before we clamp,
    // or the target could clamp short. Keep waiting until it has grown.
    if pu.upper() + 0.5 < want_h && want_h > pu.page_size() {
        return false;
    }
    let v = (y - 8.0).max(pu.lower());
    pu.set_value(v.min((pu.upper() - pu.page_size()).max(pu.lower())));
    if let Some(p) = state.borrow_mut().panes.get_mut(i) {
        p.scroll_to = None;
    }
    true
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

/// Adjust the body font: `dir` > 0 grows, < 0 shrinks, and exactly `0.0` resets
/// to the default (Ctrl+0), mirroring the browser-style zoom in the overlay.
fn zoom(state: &Shared, ui: &Ui, dir: f64) {
    {
        let mut st = state.borrow_mut();
        st.font_size = if dir == 0.0 {
            DEFAULT_FONT
        } else {
            (st.font_size + dir).clamp(MIN_FONT, MAX_FONT)
        };
    }
    for pu in ui.pane_uis.borrow().iter() {
        pu.area.queue_draw();
    }
    ui.link_layer.queue_draw();
    persist_config(state); // remember the body size across launches
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
    let links = weave::link_pairs(&weaves);
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

/// Scroll one adjustment by `px` pixels, clamped so it never overruns its range.
fn scroll_by_px(vadj: &gtk::Adjustment, px: f64) {
    let max = (vadj.upper() - vadj.page_size()).max(vadj.lower());
    vadj.set_value((vadj.value() + px).clamp(vadj.lower(), max));
}

/// Scroll every pane by the same pixel delta — the Shift-locked "read all panes
/// in parallel" gesture (overlay `scrollAllBy`).
fn scroll_all(ui: &Ui, px: f64) {
    for pu in ui.pane_uis.borrow().iter() {
        scroll_by_px(&pu.vadj, px);
    }
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

/// Re-read threads + tags + personal notes from disk after an authoring write.
fn reload_study_data(state: &Shared) {
    let home = state.borrow().home.clone();
    let (threads, _) = thread::load_threads(&home);
    let (tags, _) = tag::load_tags(&home);
    let (usernotes, _) = usernote::load_notes(&home);
    let mut st = state.borrow_mut();
    st.threads = threads;
    st.tags = tags;
    st.usernotes = usernotes;
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
    use panel::PanelLink::*;
    // The verb vocabulary is parsed once in the core (`panel::parse_link`), the
    // same source the panel producer bakes URIs from — so GTK dispatches on the
    // typed verb and can't drift from what it emits. An unknown verb is ignored.
    let Some(link) = panel::parse_link(uri) else { return };
    match link {
        Go { book, chapter, verse } => {
            // A modifier-click captured just before this (Tier 0 #8) targets the
            // other pane; otherwise the active one.
            let target = {
                let mut st = state.borrow_mut();
                let other = st.link_other && st.panes.len() > 1;
                st.link_other = false;
                if other { (st.active + 1) % st.panes.len() } else { st.active }
            };
            navigate_pane(state, ui, target, &book, chapter as u16, verse.map(|v| v as u16));
        }
        Occurrences { code } => show_study(ui, &concordance_markup(&state.borrow(), &code)),
        Rendering { code, rendering } => {
            show_study(ui, &rendering_concordance_markup(&state.borrow(), &code, &rendering));
        }
        CodeStudy { code, word } => show_study(ui, &code_study_markup(&state.borrow(), &code, &word)),
        Thread { index } => show_study(ui, &thread_markup(&state.borrow(), index)),
        Tag { index } => show_study(ui, &tag_markup(&state.borrow(), index)),
        Weave { index } => show_study(ui, &weave_markup(&state.borrow(), index)),
        ConceptMap { code } => show_concept_map(state, ui, &code),
        AddTag { refkey } => {
            if let Some(vref) = VRef::parse_ref_key(&refkey) {
                let (state, ui) = (state.clone(), ui.clone());
                let title = format!("Tag {}", vref.display());
                prompt_name(window_of(&ui), &title, "tag name (new or existing)", move |name| {
                    add_verse_to_tag(&state, &ui, &vref, &name);
                });
            }
        }
        AddThread { refkey } => {
            if let Some(vref) = VRef::parse_ref_key(&refkey) {
                let (state, ui) = (state.clone(), ui.clone());
                let title = format!("Add {} to thread", vref.display());
                prompt_name(window_of(&ui), &title, "thread name (new or existing)", move |name| {
                    add_verse_to_thread(&state, &ui, &vref, &name);
                });
            }
        }
        Untag { tag, refkey } => {
            if let Some(vref) = VRef::parse_ref_key(&refkey) {
                untag_verse(state, ui, tag, &vref);
            }
        }
        Approve { index } => review_weave(state, ui, index, true),
        Reject { index } => review_weave(state, ui, index, false),
        EditThreadNotes { index } => edit_thread_notes(state, ui, index),
        EditEntryNote { thread, entry } => edit_entry_note(state, ui, thread, entry),
        EditWeaveNotes { index } => edit_weave_notes(state, ui, index),
        EditNote { refkey } => edit_user_note(state, ui, &refkey),
        Guide => show_study(ui, &blocks_to_markup(&panel::guide_blocks())),
        About => show_study(ui, &blocks_to_markup(&panel::about_blocks())),
    }
}

/// Prompt for and save the reader's personal note on `refkey` (Tier 0 #3), then
/// refresh the gutter and re-show the verse's word study.
fn edit_user_note(state: &Shared, ui: &Ui, refkey: &str) {
    let Some(vref) = VRef::parse_ref_key(refkey) else { return };
    let current = state.borrow().usernotes.get(&vref).map(|ln| ln.note.text.clone()).unwrap_or_default();
    let (state, ui) = (state.clone(), ui.clone());
    let title = format!("Your note — {}", vref.display());
    prompt_text(window_of(&ui), &title, "note (empty clears it)", &current, move |text| {
        let res = {
            let st = state.borrow();
            usernote::set_note(&st.home, &vref, &text, &now_stamp())
        };
        match res {
            Ok(_) => {
                reload_study_data(&state);
                redraw_all(&ui);
                // Re-render the verse's word study so the "your note" line updates.
                let m = {
                    let st = state.borrow();
                    let strongs = st
                        .corpus
                        .verse(&vref)
                        .and_then(|v| v.tokens.first())
                        .map(|t| t.strongs.clone())
                        .unwrap_or_default();
                    let hit = Hit { verse: vref.clone(), token_index: 0, strongs };
                    word_study_markup(&st, &hit)
                };
                show_study(&ui, &m);
            }
            Err(e) => show_study(&ui, &format!("<i>Could not save note: {}</i>", esc(&e.to_string()))),
        }
    });
}

/// The verse under a reader point: the hit word's verse, else the nearest
/// verse-number line by y (Tier 0 #1 — a right-click anywhere in a verse's
/// lines targets that verse). `None` when no chapter is laid out.
fn verse_at(state: &Shared, i: usize, x: f64, y: f64) -> Option<VRef> {
    let st = state.borrow();
    let p = st.panes.get(i)?;
    let dl = p.dl.as_ref()?;
    if let Some(hit) = dl.hit_test(x as f32 - p.margin_x, y as f32 - MARGIN) {
        return Some(hit.verse);
    }
    let ty = y as f32 - MARGIN;
    let mut best: Option<(f32, u16)> = None;
    for it in &dl.items {
        if let Some(n) = item_verse_num(it) {
            let d = (it.y - ty).abs();
            if best.map_or(true, |(bd, _)| d < bd) {
                best = Some((d, n));
            }
        }
    }
    best.map(|(_, n)| VRef::new(&p.book, p.chapter, n))
}

/// Copy a verse (or its chapter) to the clipboard in one shape (Tier 0 #1).
fn copy_verse(state: &Shared, area: &gtk::DrawingArea, vref: &VRef, kind: export::CopyKind) {
    let text = export::copy_text(&state.borrow().corpus, vref, kind);
    if let Some(t) = text {
        area.clipboard().set_text(&t);
    }
}

/// Highlight a verse with a named tone: add it to that colour's tag (creating it
/// coloured), then reload + repaint (Tier 0 #4).
fn highlight_verse(state: &Shared, ui: &Ui, vref: &VRef, tone: &str, hex: &str) {
    // "amber" → "Amber": the tag name for this tone.
    let mut chars = tone.chars();
    let name = match chars.next() {
        Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
        None => return,
    };
    let home = state.borrow().home.clone();
    let stamp = now_stamp();
    let added = {
        let st = state.borrow();
        tag::add_member(
            &home,
            &st.tags,
            &name,
            canon::TOKENIZATION_VERSION,
            TagTarget::Verse(vref.clone()),
            None,
            &stamp,
        )
    };
    if added.is_ok() {
        reload_study_data(state);
        {
            let st = state.borrow();
            let _ = tag::set_color(&st.tags, &name, Some(hex));
        }
        reload_study_data(state);
        redraw_all(ui);
    }
}

/// Persist a word-precise cross-verse highlight (Tier 0 #4 drag) under a named
/// tone, then reload + repaint. Endpoints are ordered canonically, so a
/// backwards drag stores the same range.
fn highlight_range(
    state: &Shared,
    ui: &Ui,
    sref: &VRef,
    stok: u32,
    eref: &VRef,
    etok: u32,
    tone: &str,
    hex: &str,
) {
    // "amber" → "Amber": the tag name for this tone (matches highlight_verse).
    let mut chars = tone.chars();
    let name = match chars.next() {
        Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
        None => return,
    };
    let ((s, s_tok), (e, e_tok)) = if (sref.reading_key(), stok) <= (eref.reading_key(), etok) {
        ((sref.clone(), stok), (eref.clone(), etok))
    } else {
        ((eref.clone(), etok), (sref.clone(), stok))
    };
    let clamp = |t: u32| t.min(u16::MAX as u32) as u16;
    let stamp = now_stamp();
    let range = tag::HighlightRange {
        start: s,
        start_tok: clamp(s_tok),
        end: e,
        end_tok: clamp(e_tok),
        color: Some(hex.to_string()),
        note: None,
        added: stamp.clone(),
    };
    let home = state.borrow().home.clone();
    let res = {
        let st = state.borrow();
        tag::add_highlight(&home, &st.tags, &name, canon::TOKENIZATION_VERSION, range, &stamp)
    };
    if res.is_ok() {
        reload_study_data(state);
        redraw_all(ui);
    }
}

/// Remove a verse from every colour-bearing tag that holds it, and drop any
/// word-precise highlight range that covers it (Tier 0 #4). Each affected tag is
/// rewritten once, so member and range clears never clobber one another.
fn clear_highlight(state: &Shared, ui: &Ui, vref: &VRef) {
    let target = TagTarget::Verse(vref.clone());
    let rk = vref.reading_key();
    let covers = |h: &tag::HighlightRange| h.start.reading_key() <= rk && rk <= h.end.reading_key();
    let affected: Vec<LoadedTag> = {
        let st = state.borrow();
        st.tags
            .iter()
            .filter(|lt| {
                (lt.tag.color.is_some() && lt.tag.member_of(&target))
                    || lt.tag.highlights.iter().any(covers)
            })
            .cloned()
            .collect()
    };
    for lt in &affected {
        let mut t = lt.tag.clone();
        // Only colour-bearing tags wash whole verses; leave a plain semantic
        // tag's membership intact and just drop its covering ranges.
        if t.color.is_some() {
            t.members.retain(|m| m.target != target);
        }
        t.highlights.retain(|h| !covers(h));
        let _ = tag::write_tag(&lt.file, &t);
    }
    if !affected.is_empty() {
        reload_study_data(state);
        redraw_all(ui);
    }
}

/// The right-click verse context menu (Tier 0 #1): copy shapes, highlight tones,
/// a personal note, and (Full study) tag / add-to-thread — the last three route
/// through the panel dispatcher so they share the prompts + authoring flow.
fn show_context_menu(state: &Shared, ui: &Ui, i: usize, area: &gtk::DrawingArea, x: f64, y: f64) {
    let Some(vref) = verse_at(state, i, x, y) else { return };
    let refkey = vref.ref_key();
    let full = state.borrow().mode.is_full();

    let pop = gtk::Popover::new();
    pop.set_parent(area);
    pop.set_has_arrow(false);
    pop.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 2);
    vbox.set_margin_top(4);
    vbox.set_margin_bottom(4);
    vbox.set_margin_start(4);
    vbox.set_margin_end(4);

    let make = |label: &str| {
        let b = gtk::Button::with_label(label);
        b.add_css_class("flat");
        b.set_halign(gtk::Align::Fill);
        b
    };

    // Copy shapes.
    for (label, kind) in [
        ("Copy verse", export::CopyKind::Verse),
        ("Copy with reference", export::CopyKind::VerseRef),
        ("Copy (markdown)", export::CopyKind::VerseMarkdown),
        ("Copy chapter", export::CopyKind::Chapter),
    ] {
        let b = make(label);
        let (state, area, vref, pop) = (state.clone(), area.clone(), vref.clone(), pop.clone());
        b.connect_clicked(move |_| {
            copy_verse(&state, &area, &vref, kind);
            pop.popdown();
        });
        vbox.append(&b);
    }
    vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // Personal note (both modes).
    {
        let b = make("Note…");
        let (state, ui, refkey, pop) = (state.clone(), ui.clone(), refkey.clone(), pop.clone());
        b.connect_clicked(move |_| {
            handle_link(&state, &ui, &format!("editnote:{refkey}"));
            pop.popdown();
        });
        vbox.append(&b);
    }

    // Highlight tones + clear.
    for (tone, hex) in theme::HIGHLIGHT_TONES {
        let b = make(&format!("Highlight — {tone}"));
        let (state, ui, vref, pop) = (state.clone(), ui.clone(), vref.clone(), pop.clone());
        b.connect_clicked(move |_| {
            highlight_verse(&state, &ui, &vref, tone, hex);
            pop.popdown();
        });
        vbox.append(&b);
    }
    {
        let b = make("Remove highlight");
        let (state, ui, vref, pop) = (state.clone(), ui.clone(), vref.clone(), pop.clone());
        b.connect_clicked(move |_| {
            clear_highlight(&state, &ui, &vref);
            pop.popdown();
        });
        vbox.append(&b);
    }

    // Tag / add-to-thread (Full study only), routed through the dispatcher.
    if full {
        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        for (label, verb) in [("Tag…", "addtag"), ("Add to thread…", "addthread")] {
            let b = make(label);
            let (state, ui, refkey, pop) = (state.clone(), ui.clone(), refkey.clone(), pop.clone());
            b.connect_clicked(move |_| {
                handle_link(&state, &ui, &format!("{verb}:{refkey}"));
                pop.popdown();
            });
            vbox.append(&b);
        }
    }

    pop.set_child(Some(&vbox));
    pop.popup();
}

/// Apply a specific colour theme (light · dark · night · follow system), re-theme
/// the chrome + CSS + canvases, and persist (Tier 0 #5). Driven by the win.theme
/// menu radio.
fn apply_theme_choice(state: &Shared, ui: &Ui, choice: theme::ThemeChoice) {
    let sm = adw::StyleManager::default();
    state.borrow_mut().theme_choice = choice;
    // System follows the OS scheme; the explicit choices force it.
    match choice {
        theme::ThemeChoice::System => sm.set_color_scheme(adw::ColorScheme::Default),
        theme::ThemeChoice::Light => sm.set_color_scheme(adw::ColorScheme::ForceLight),
        _ => sm.set_color_scheme(adw::ColorScheme::ForceDark),
    }
    let palette = theme::palette(choice.resolve(sm.is_dark()));
    state.borrow_mut().palette = palette.clone();
    set_markup_palette(&palette);
    ui.css.load_from_data(&css_string(&palette));
    persist_config(state);
    redraw_all(ui);
    ui.canon_map.queue_draw();
    ui.link_layer.queue_draw();
}

/// Build the primary (≡) menu model: weave views, reading, theme, and help.
/// Backed by the win.* GActions installed on the window.
fn build_primary_menu() -> gio::Menu {
    let menu = gio::Menu::new();

    let views = gio::Menu::new();
    views.append(Some("Suggested weaves"), Some("win.suggested"));
    views.append(Some("Weave map"), Some("win.weave-map"));
    views.append(Some("Constellation"), Some("win.constellation"));
    menu.append_submenu(Some("Weave views"), &views);

    let reading = gio::Menu::new();
    let modes = gio::Menu::new();
    modes.append(Some("Simple reader"), Some("win.mode::simple"));
    modes.append(Some("Full study"), Some("win.mode::full"));
    reading.append_section(None, &modes);
    let vpl = gio::Menu::new();
    vpl.append(Some("Verse per line"), Some("win.verse-per-line"));
    reading.append_section(None, &vpl);
    menu.append_submenu(Some("Reading"), &reading);

    let themes = gio::Menu::new();
    themes.append(Some("Light"), Some("win.theme::light"));
    themes.append(Some("Dark"), Some("win.theme::dark"));
    themes.append(Some("Night"), Some("win.theme::night"));
    themes.append(Some("Follow system"), Some("win.theme::system"));
    menu.append_submenu(Some("Theme"), &themes);

    let help = gio::Menu::new();
    help.append(Some("Guide"), Some("win.guide"));
    help.append(Some("Keyboard shortcuts"), Some("win.shortcuts"));
    help.append(Some("About pure-study"), Some("win.about"));
    menu.append_section(None, &help);

    menu
}

/// The keyboard-shortcuts overlay (Tier 0 #7), a small modal window.
fn show_shortcuts(_state: &Shared, ui: &Ui) {
    let win = gtk::Window::builder().title("Keyboard shortcuts").modal(true).default_width(480).build();
    if let Some(p) = window_of(ui) {
        win.set_transient_for(Some(&p));
    }
    let grid = gtk::Grid::new();
    grid.set_row_spacing(6);
    grid.set_column_spacing(20);
    grid.set_margin_top(18);
    grid.set_margin_bottom(18);
    grid.set_margin_start(18);
    grid.set_margin_end(18);
    let rows = [
        ("↑ / ↓ / Space", "scroll"),
        ("PageUp / PageDown", "scroll a page"),
        ("Home / End", "chapter start / end"),
        ("← / →  (or [ / ])", "step chapters, across books"),
        ("Alt + ← / →", "back / forward in history"),
        ("Mouse back / forward", "back / forward in history"),
        ("Shift + scroll", "lock all panes together"),
        ("Ctrl + scroll, Ctrl +/−", "zoom · Ctrl 0 resets"),
        ("Ctrl + click / double-click", "word study"),
        ("Right-click a verse", "copy · note · highlight · tag"),
        ("Esc", "close the panel / a popup"),
        ("F1 / ?", "this list"),
    ];
    for (r, (k, v)) in rows.iter().enumerate() {
        let key = gtk::Label::new(Some(k));
        key.set_xalign(0.0);
        key.add_css_class("heading");
        let act = gtk::Label::new(Some(v));
        act.set_xalign(0.0);
        act.set_wrap(true);
        grid.attach(&key, 0, r as i32, 1, 1);
        grid.attach(&act, 1, r as i32, 1, 1);
    }
    win.set_child(Some(&grid));
    let keyc = gtk::EventControllerKey::new();
    let w2 = win.clone();
    keyc.connect_key_pressed(move |_c, key, _code, _m| {
        if key == gdk::Key::Escape {
            w2.close();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    win.add_controller(keyc);
    win.present();
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

/// Parse a `#rrggbb` palette hex into cairo's 0..1 `(r, g, b)`. Falls back to a
/// near-black on a malformed value.
fn hex_rgb(hex: &str) -> (f64, f64, f64) {
    let h = hex.trim_start_matches('#');
    let c = |a: usize, b: usize| u8::from_str_radix(h.get(a..b).unwrap_or("00"), 16).unwrap_or(0) as f64 / 255.0;
    if h.len() >= 6 { (c(0, 2), c(2, 4), c(4, 6)) } else { (0.1, 0.1, 0.1) }
}

/// Lay out and paint pane `i`'s chapter. Measurement and painting share one
/// Pango layout, so the stored hit regions match the glyphs exactly.
fn draw_pane(state: &Shared, i: usize, area: &gtk::DrawingArea, cr: &cairo::Context, width: i32) {
    let mut st = state.borrow_mut();
    let pal = st.palette.clone();
    // Paper background from the theme (warm cream in light; dark in dark/night).
    let (pr0, pg0, pb0) = hex_rgb(&pal.paper);
    cr.set_source_rgb(pr0, pg0, pb0);
    let _ = cr.paint();

    if i >= st.panes.len() {
        return;
    }
    let family = st.family.clone();
    let font_size = st.font_size;
    let book = st.panes[i].book.clone();
    let chapter = st.panes[i].chapter;
    let highlight = st.panes[i].highlight;
    let pin = st.panes[i].pin;
    let hl_drag = st.panes[i].hl_drag.clone();

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
        verse_break: st.verse_per_line,
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
    // Verses with a personal note get a second gutter mark (Tier 0 #3).
    let note_here: HashSet<u16> = verses
        .iter()
        .map(|v| v.verse)
        .filter(|&n| st.usernotes.contains_key(&VRef::new(&book, chapter, n)))
        .collect();
    // Verses that are search hits get a soft band (Tier 0 #8).
    let hits_here: Vec<u16> = verses
        .iter()
        .map(|v| v.verse)
        .filter(|&n| st.hits.contains(&VRef::new(&book, chapter, n)))
        .collect();
    // Verses in a colour-bearing tag get a highlight wash (Tier 0 #4).
    let highlight_here: Vec<(u16, (f64, f64, f64))> = verses
        .iter()
        .filter_map(|v| {
            tag::verse_color(&st.tags, &VRef::new(&book, chapter, v.verse))
                .map(|hex| (v.verse, hex_rgb(hex)))
        })
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
    let (gr, gg, gb) = hex_rgb(&pal.gold);
    // Highlight washes first (underneath), then search hits, then the goto band.
    for (vn, (r, g, b)) in &highlight_here {
        band(cr, *vn, *r, *g, *b, if pal.dark { 0.25 } else { 0.36 });
    }

    // Word-precise highlight runs — cross-verse drag highlights (Tier 0 #4).
    // Each run is a [lo,hi] token span in a verse, painted like the pin span so
    // the wash follows the actual words (partial first/last verse, whole middle).
    let run_alpha = if pal.dark { 0.25 } else { 0.36 };
    let paint_run = |cr: &cairo::Context, vref: &VRef, lo: u32, hi: u32, rgb: (f64, f64, f64), alpha: f64| {
        let (r, g, b) = rgb;
        cr.set_source_rgba(r, g, b, alpha);
        for it in &dl.items {
            if let Some((wv, t)) = it.word() {
                if wv == vref && t >= lo && t <= hi {
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
    };
    for v in &verses {
        let vref = VRef::new(&book, chapter, v.verse);
        let len = v.tokens.len().min(u16::MAX as usize) as u16;
        for run in tag::verse_highlight_runs(&st.tags, &vref, len) {
            paint_run(cr, &vref, run.lo as u32, run.hi as u32, hex_rgb(&run.color), run_alpha);
        }
    }
    // Live preview while dragging: the same decomposition as the final wash, but
    // a bolder alpha so the selection's full extent (whole in-between verses
    // included — it's a text selection) is obvious before release. The highlight
    // lands exactly where the preview shows.
    if let Some((sr, stok, er, etok)) = &hl_drag {
        let (lo_end, hi_end) = {
            let (a, b) = ((sr.reading_key(), *stok), (er.reading_key(), *etok));
            if a <= b { (a, b) } else { (b, a) }
        };
        let drag_rgb = hex_rgb(theme::HIGHLIGHT_TONES[0].1);
        let preview_alpha = (run_alpha + 0.30).min(0.66);
        for v in &verses {
            let vref = VRef::new(&book, chapter, v.verse);
            let rk = vref.reading_key();
            if rk < lo_end.0 || rk > hi_end.0 {
                continue;
            }
            let last = v.tokens.len().saturating_sub(1) as u32;
            let lo = if rk == lo_end.0 { lo_end.1 } else { 0 };
            let hi = if rk == hi_end.0 { hi_end.1 } else { last };
            if lo <= hi {
                paint_run(cr, &vref, lo, hi, drag_rgb, preview_alpha);
            }
        }
    }

    for &n in &hits_here {
        band(cr, n, gr, gg, gb, 0.12);
    }
    if let Some(hv) = highlight {
        band(cr, hv, gr, gg, gb, 0.12);
    }
    // A pinned endpoint highlights its exact word span (blue), so the reader
    // sees which words the link will point at — not just the whole verse.
    if let Some(ps) = pin {
        let pv = VRef::new(&book, chapter, ps.verse);
        let (pnr, png, pnb) = hex_rgb(&pal.pin);
        cr.set_source_rgba(pnr, png, pnb, 0.22);
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
                cr.set_source_rgb(gr, gg, gb); // gold
                cr.move_to(px, py);
                layout.set_text(&item.text);
                pangocairo::functions::show_layout(cr, &layout);
                if xref_here.contains(n) {
                    cr.set_source_rgba(gr, gg, gb, 0.75);
                    cr.arc((margin_x as f64 - 9.0).max(3.0), baseline - 4.0, 2.3, 0.0, std::f64::consts::TAU);
                    let _ = cr.fill();
                }
                // A square note mark, left of the xref dot (Tier 0 #3).
                if note_here.contains(n) {
                    let (fr, fg, fb) = hex_rgb(&pal.faded);
                    cr.set_source_rgb(fr, fg, fb);
                    let _ = cr.rectangle((margin_x as f64 - 13.5).max(1.0), baseline - 8.0, 3.2, 3.2);
                    let _ = cr.fill();
                }
            }
            ItemKind::Word { .. } => {
                if item.flags & FLAG_ADDED != 0 {
                    layout.set_font_description(Some(&italic));
                    let (r, g, b) = hex_rgb(&pal.added);
                    cr.set_source_rgb(r, g, b);
                } else {
                    layout.set_font_description(Some(&regular));
                    let (r, g, b) = if item.flags & FLAG_DIVINE != 0 {
                        hex_rgb(&pal.divine)
                    } else if item.flags & FLAG_TITLE != 0 {
                        hex_rgb(&pal.title_ink)
                    } else {
                        hex_rgb(&pal.ink)
                    };
                    cr.set_source_rgb(r, g, b);
                }
                cr.move_to(px, py);
                layout.set_text(&item.text);
                pangocairo::functions::show_layout(cr, &layout);

                if !item.strongs.is_empty() {
                    cr.set_source_rgba(gr, gg, gb, 0.30);
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
fn draw_links(state: &Shared, ui: &Ui, layer: &gtk::DrawingArea, cr: &cairo::Context, _w: i32, _h: i32) {
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
        // A link within one pane has no gap to span — skip it.
        if pa == pb {
            continue;
        }
        // The connectors ride the *gutter* between panes, not the text: anchor
        // each end at its pane's inner edge (right edge of the left pane, left
        // edge of the right pane) inset a little, at the verse's own height —
        // so a line never crosses a column of text (overlay ReaderView).
        let (Some(ya), Some(yb)) = (
            link_verse_y(&st, &pus, pa, a.verse, layer),
            link_verse_y(&st, &pus, pb, b.verse, layer),
        ) else {
            continue;
        };
        let (Some((la, ra)), Some((lb, rb))) =
            (pane_x_edges(&pus, pa, layer), pane_x_edges(&pus, pb, layer))
        else {
            continue;
        };
        // Whichever pane sits to the left anchors on its right edge; the other
        // on its left edge. Inset so the dot sits just inside the margin.
        let (xa, xb) = if la <= lb {
            (ra - LINK_INSET, lb + LINK_INSET)
        } else {
            (la + LINK_INSET, rb - LINK_INSET)
        };
        // A verse scrolled out of view doesn't drop its connector — clamp the
        // endpoint to the pane's visible scripture band so a dot lingers at the
        // top/bottom edge as a hint you can scroll it into view (overlay clampY).
        let (Some((ta, ba)), Some((tb, bb))) =
            (pane_y_band(&pus, pa, layer), pane_y_band(&pus, pb, layer))
        else {
            continue;
        };
        let ya = ya.clamp(ta + LINK_YINSET, (ba - LINK_YINSET).max(ta + LINK_YINSET));
        let yb = yb.clamp(tb + LINK_YINSET, (bb - LINK_YINSET).max(tb + LINK_YINSET));
        let dx = xb - xa;
        cr.set_source_rgba(0.62, 0.49, 0.22, 0.35);
        cr.move_to(xa, ya);
        cr.curve_to(xa + dx * 0.4, ya, xb - dx * 0.4, yb, xb, yb);
        let _ = cr.stroke();
        for (x, y) in [(xa, ya), (xb, yb)] {
            cr.set_source_rgba(0.62, 0.49, 0.22, 0.7);
            cr.arc(x, y, 2.0, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
        }
    }
}

/// A weave connector's inset from the pane's inner edge (overlay uses 14px), so
/// the endpoint dot sits just inside the gutter rather than flush on the border.
const LINK_INSET: f64 = 14.0;
/// Vertical inset when clamping an off-screen endpoint to the visible band, so
/// the hint dot sits just inside the top/bottom edge rather than half-clipped.
const LINK_YINSET: f64 = 5.0;

/// The top and bottom of pane `pane`'s *visible* scripture band, in `layer`
/// coordinates — the current scroll window, used to pin off-screen connector
/// endpoints to the edge as a scroll hint.
fn pane_y_band(pus: &[PaneUi], pane: usize, layer: &gtk::DrawingArea) -> Option<(f64, f64)> {
    let pu = pus.get(pane)?;
    let top = pu.vadj.value();
    let bot = top + pu.vadj.page_size();
    let t = pu.area.compute_point(layer, &gtk::graphene::Point::new(0.0, top as f32))?;
    let b = pu.area.compute_point(layer, &gtk::graphene::Point::new(0.0, bot as f32))?;
    Some((t.y() as f64, b.y() as f64))
}

/// The y, in `layer` coordinates, of `verse`'s line in pane `pane` — or `None`
/// if the pane hasn't painted or lacks that verse. (x is irrelevant: the
/// transform has no rotation, so any column x maps to the same layer y.)
fn link_verse_y(
    st: &State,
    pus: &[PaneUi],
    pane: usize,
    verse: u16,
    layer: &gtk::DrawingArea,
) -> Option<f64> {
    let p = st.panes.get(pane)?;
    let dl = p.dl.as_ref()?;
    let it = dl
        .items
        .iter()
        .find(|it| matches!(it.kind, ItemKind::VerseNumber(n) if n == verse))?;
    let py = MARGIN + it.y + it.h * 0.5;
    let pt = pus.get(pane)?.area.compute_point(layer, &gtk::graphene::Point::new(0.0, py))?;
    Some(pt.y() as f64)
}

/// The left and right edges of pane `pane`'s canvas, in `layer` coordinates.
fn pane_x_edges(pus: &[PaneUi], pane: usize, layer: &gtk::DrawingArea) -> Option<(f64, f64)> {
    let area = &pus.get(pane)?.area;
    let w = area.width() as f32;
    let l = area.compute_point(layer, &gtk::graphene::Point::new(0.0, 0.0))?;
    let r = area.compute_point(layer, &gtk::graphene::Point::new(w, 0.0))?;
    Some((l.x() as f64, r.x() as f64))
}

/// Draw the canon-overview strip: the eight canon sections across the 66 books,
/// the OT/NT divide, and a pin per pane at its current book (active in gold).
fn draw_canon(state: &Shared, cr: &cairo::Context, w: i32, h: i32) {
    let st = state.borrow();
    let width = w as f64;
    let hf = h as f64;
    let nb = canon::BOOKS.len() as f64;
    let pal = &st.palette;
    let (gr, gg, gb) = hex_rgb(&pal.gold);
    let (fr, fg, fb) = hex_rgb(&pal.faded);

    let (sr, sg, sb) = hex_rgb(&pal.strip_bg);
    cr.set_source_rgb(sr, sg, sb);
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
            if pal.dark {
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.06);
            } else {
                cr.set_source_rgba(0.0, 0.0, 0.0, 0.04);
            }
            let _ = cr.rectangle(x0, 0.0, x1 - x0, hf);
            let _ = cr.fill();
        }
        layout.set_text(lbl);
        let (tw, th) = layout.pixel_size();
        if (tw as f64) < (x1 - x0) - 6.0 {
            let (lr, lg, lb) = hex_rgb(&pal.section);
            cr.set_source_rgba(lr, lg, lb, 0.95);
            cr.move_to((x0 + x1) / 2.0 - tw as f64 / 2.0, hf / 2.0 - th as f64 / 2.0 - 2.0);
            pangocairo::functions::show_layout(cr, &layout);
        }
    }

    let dx = OT_NT_DIVIDE as f64 / nb * width;
    cr.set_source_rgba(fr, fg, fb, 0.55);
    cr.set_line_width(1.0);
    cr.move_to(dx, 0.0);
    cr.line_to(dx, hf);
    let _ = cr.stroke();

    for (i, p) in st.panes.iter().enumerate() {
        let bi = canon::book_order(&p.book).unwrap_or(0) as f64;
        let x = (bi + 0.5) / nb * width;
        if i == st.active {
            cr.set_source_rgb(gr, gg, gb);
        } else {
            cr.set_source_rgba(fr, fg, fb, 0.6);
        }
        cr.arc(x, hf - 4.0, 3.5, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
    }
}

/// Aggregate the loaded weave links into book-pair ribbons (canon-ordered book
/// indices → link count), plus the max count for scaling. Ported from
/// `ChordMap` aggregation.
fn chord_arcs(st: &State) -> (Vec<(usize, usize, u32)>, u32) {
    // The book-pair fold lives once in the core (`weave::chord_pairs`), shared
    // with the non-Rust shells via `pure_engine_chord_map_json`. GTK calls it
    // directly; the drawing below is the only shell-side part.
    weave::chord_pairs(&st.weaves)
}

/// A filled arc ribbon from foot A to foot B over `apex`, in the given colour.
fn arc_ribbon(cr: &cairo::Context, y0: f64, fa: (f64, f64), fb: (f64, f64), apex: f64, col: (f64, f64, f64, f64)) {
    cr.set_source_rgba(col.0, col.1, col.2, col.3);
    cr.move_to(fa.0, y0);
    cr.curve_to(fa.0, y0 - apex, fb.1, y0 - apex, fb.1, y0);
    cr.line_to(fb.0, y0);
    cr.curve_to(fb.0, y0 - apex * 0.82, fa.1, y0 - apex * 0.82, fa.1, y0);
    cr.close_path();
    let _ = cr.fill();
}

/// The book-to-book chord/arc map: how strongly each book pair is woven, drawn
/// over the same canon-ordered axis as the canon strip. Ribbon colour marks
/// OT-internal / NT-internal / cross-testament (the interesting case). Ported
/// from `ChordMap.chordMapView`.
fn draw_chord_map(state: &Shared, cr: &cairo::Context, w: i32, h: i32) {
    let st = state.borrow();
    let (arcs, max_c) = chord_arcs(&st);
    let width = w as f64;
    let hf = h as f64;
    let nb = canon::BOOKS.len() as f64;
    let y0 = hf - 26.0;
    let axis_h = y0 - 8.0;
    let x_at = |f: f64| f.clamp(0.0, 1.0) * width;
    let book_x = |i: usize| x_at((i as f64 + 0.5) / nb);

    // warm paper backdrop
    cr.set_source_rgb(0.949, 0.933, 0.902);
    let _ = cr.rectangle(0.0, 0.0, width, hf);
    let _ = cr.fill();

    // section bands + labels
    let layout = pangocairo::functions::create_layout(cr);
    let mut fd = pango::FontDescription::new();
    fd.set_family(&st.family);
    fd.set_absolute_size(10.0 * pango::SCALE as f64);
    layout.set_font_description(Some(&fd));
    for (k, (lbl, lo, hi)) in CANON_SEGMENTS.iter().enumerate() {
        let x0 = *lo as f64 / nb * width;
        let x1 = (*hi + 1) as f64 / nb * width;
        if k % 2 == 1 {
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.04);
            let _ = cr.rectangle(x0, 0.0, x1 - x0, y0);
            let _ = cr.fill();
        }
        cr.set_source_rgba(0.35, 0.30, 0.22, 0.9);
        cr.move_to(x0 + 3.0, y0 + 6.0);
        layout.set_text(lbl);
        pangocairo::functions::show_layout(cr, &layout);
    }

    // baseline + OT/NT seam
    cr.set_source_rgba(0.62, 0.49, 0.22, 0.5);
    let _ = cr.rectangle(0.0, y0, width, 1.5);
    let _ = cr.fill();
    let dx = OT_NT_DIVIDE as f64 / nb * width;
    let _ = cr.rectangle(dx - 0.5, 0.0, 1.0, y0);
    let _ = cr.fill();

    // ribbons, heaviest first so thin ones stay visible on top
    let mut arcs = arcs;
    arcs.sort_by(|a, b| b.2.cmp(&a.2));
    for (a, b, cnt) in arcs {
        let al = 0.12 + 0.30 * (cnt as f64 / max_c as f64);
        let col = if a < OT_NT_DIVIDE && b < OT_NT_DIVIDE {
            (0.82, 0.70, 0.43, al) // OT gold
        } else if a >= OT_NT_DIVIDE && b >= OT_NT_DIVIDE {
            (0.50, 0.70, 0.90, al) // NT blue
        } else {
            (0.78, 0.59, 0.86, (al + 0.08).min(0.5)) // cross-testament
        };
        let wd = 2.0 + 8.0 * (cnt as f64 / max_c as f64);
        let (xa, xb) = (book_x(a), book_x(b));
        if a == b {
            arc_ribbon(cr, y0, (xa - wd, xa), (xa, xa + wd), (wd * 1.4).max(10.0), col);
        } else {
            let apex = 24.0 + (axis_h - 74.0).max(0.0) * (((xb - xa).abs() / width) as f64).powf(0.75);
            arc_ribbon(cr, y0, (xa - wd / 2.0, xa + wd / 2.0), (xb - wd / 2.0, xb + wd / 2.0), apex, col);
        }
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
// ── the study-panel content model ─────────────────────────────────────────────
//
// The panel derivation lives once in `pure_core::panel`; `State` implements the
// `PanelSource` trait so the GTK shell calls that producer directly (Rust→Rust,
// the same blocks the non-Rust shells get as JSON), and `blocks_to_markup` is
// the whole per-block painter — one Pango markup string for the panel GtkLabel.
// Colours come from the semantic role; link runs become <a href> (GTK styles
// them); sizes are the block model's logical points.

impl PanelSource for State {
    fn token_word(&self, verse: &str, token: u32) -> Option<String> {
        let v = VRef::parse_ref_key(verse)?;
        self.corpus.verse(&v)?.tokens.get(token as usize).map(|t| t.word.clone())
    }
    fn verse_display(&self, refkey: &str) -> Option<String> {
        VRef::parse_ref_key(refkey).map(|v| v.display())
    }
    fn morph_gloss(&self, verse: &str, token: u32) -> Option<String> {
        let (md, v) = (self.morph.as_ref()?, VRef::parse_ref_key(verse)?);
        md.gloss(&v, token)
    }
    fn occurrence_count(&self, code: &str) -> usize {
        self.occ_ix.verses(code).len()
    }
    fn strongs(&self, code: &str) -> Option<panel::StrongsView> {
        let e = self.strongs.get(code)?;
        Some(panel::StrongsView {
            lemma: e.lemma.clone(),
            xlit: e.xlit.clone(),
            pron: e.pron.clone(),
            deriv: e.deriv.clone(),
            def: e.def.clone(),
            kjv: e.kjv.clone(),
        })
    }
    fn gloss(&self, code: &str) -> Option<String> {
        english_gloss(self, code)
    }
    fn chip(&self, code: &str) -> panel::ChipView {
        panel::ChipView {
            code: code.to_string(),
            gloss: english_gloss(self, code),
            lemma: self.strongs.get(code).and_then(|e| e.lemma.clone()),
        }
    }
    fn renderings(&self, code: &str) -> Vec<panel::RenderingView> {
        self.renderings
            .renderings(code)
            .into_iter()
            .map(|r| panel::RenderingView { rendering: r.label.to_string(), total: r.count as u32 })
            .collect()
    }
    fn rendering_refs(&self, code: &str, rendering: &str) -> Option<panel::RenderingRefsView> {
        let key = pure_core::renderings::normalize(rendering);
        let r = self.renderings.renderings(code).into_iter().find(|r| pure_core::renderings::normalize(r.label) == key)?;
        Some(panel::RenderingRefsView {
            rendering: r.label.to_string(),
            total: r.count as u32,
            refs: r.occs.iter().take(PANEL_OCC_CAP).map(|o| (o.vref.ref_key(), o.vref.display())).collect(),
        })
    }
    fn word_codes(&self, word: &str) -> Vec<String> {
        self.renderings.word_codes(word).into_iter().map(|(c, _)| c.to_string()).collect()
    }
    fn occurrences(&self, code: &str) -> panel::OccurrencesView {
        let all = self.occ_ix.verses(code);
        panel::OccurrencesView {
            total: all.len() as u32,
            verses: all.iter().take(PANEL_OCC_CAP).map(|v| (v.ref_key(), v.display())).collect(),
        }
    }
    fn bridge_partners(&self, code: &str) -> Vec<panel::BridgePartnerView> {
        self.bridge
            .partners(code)
            .into_iter()
            .map(|p| panel::BridgePartnerView {
                sources: p.sources.iter().map(|s| bridge::source_label(s).to_string()).collect(),
                tiers: bridge::tiers_of(&p.sources).into_iter().map(|t| t.wire_name().to_string()).collect(),
                research_grade: p.sources.iter().any(|s| bridge::research_grade(s)),
                code: p.code,
            })
            .collect()
    }
    fn concept_near(&self, code: &str, k: usize) -> (Vec<String>, Vec<String>) {
        match self.embedding.as_ref() {
            Some(emb) => (
                emb.nearest_concepts(code, k).into_iter().map(|(c, _)| c).collect(),
                emb.cross_concepts(code, k).into_iter().map(|(c, _)| c).collect(),
            ),
            None => (Vec::new(), Vec::new()),
        }
    }
    fn concept(&self, code: &str) -> Option<panel::ConceptView> {
        let ce = self.concept.as_ref()?;
        ce.stat(code)?;
        let (ot, nt) = ce.testament_split(code);
        let leitwort = self.leitwort.as_ref().and_then(|m| m.get(code)).map(|b| panel::LeitwortView {
            n: b.n,
            win_count: b.win_count,
            score: b.score,
            label: burst::span_label(|id| canon::display_name(id).to_string(), &b.win_start, &b.win_end),
        });
        Some(panel::ConceptView {
            community: ce.community(code),
            top_books: ce.top_books(code, 5).into_iter().map(|(b, n)| (canon::display_name(&b).to_string(), n)).collect(),
            ot,
            nt,
            leitwort,
        })
    }
    fn verse_xrefs(&self, verse: &str) -> Vec<panel::XrefView> {
        let Some(v) = VRef::parse_ref_key(verse) else { return Vec::new() };
        match self.xrefs.get(&v) {
            Some(xs) => xs
                .iter()
                .map(|x| panel::XrefView {
                    verse: x.partner.ref_key(),
                    display: x.partner.display(),
                    weave: x.weave.clone(),
                    weave_index: self.weaves.iter().position(|lw| lw.weave.name == x.weave),
                })
                .collect(),
            None => Vec::new(),
        }
    }
    fn study_xrefs(&self, verse: &str) -> Vec<panel::StudyXrefView> {
        let Some(v) = VRef::parse_ref_key(verse) else { return Vec::new() };
        match self.xref_ix.get(&v) {
            Some(rs) => rs
                .iter()
                .map(|r| panel::StudyXrefView {
                    to: r.to.ref_key(),
                    to_display: r.to.display(),
                    end: r.end.as_ref().map(|e| e.ref_key()),
                    end_display: r.end.as_ref().map(|e| e.display()),
                })
                .collect(),
            None => Vec::new(),
        }
    }
    fn similar_verses(&self, verse: &str, k: usize) -> (Vec<panel::SimilarView>, Vec<panel::SimilarView>) {
        let (Some(vs), Some(v)) = (self.verse_sim.as_ref(), VRef::parse_ref_key(verse)) else {
            return (Vec::new(), Vec::new());
        };
        let map = |items: Vec<(VRef, f32)>| {
            items.into_iter().map(|(r, _)| panel::SimilarView { verse: r.ref_key(), display: r.display() }).collect()
        };
        (map(vs.similar_verses_in(&v, k)), map(vs.similar_verses_cross(&v, k)))
    }
    fn verse_tags(&self, verse: &str) -> Vec<(usize, String)> {
        let Some(v) = VRef::parse_ref_key(verse) else { return Vec::new() };
        let vt = TagTarget::Verse(v);
        self.tags.iter().enumerate().filter(|(_, lt)| lt.tag.member_of(&vt)).map(|(i, lt)| (i, lt.tag.name.clone())).collect()
    }
    fn verse_notes(&self, verse: &str) -> Vec<String> {
        let Some(v) = VRef::parse_ref_key(verse) else { return Vec::new() };
        self.notes.get(&v).cloned().unwrap_or_default()
    }
    fn user_note(&self, verse: &str) -> Option<String> {
        let v = VRef::parse_ref_key(verse)?;
        self.usernotes.get(&v).map(|ln| ln.note.text.clone())
    }
    fn threads(&self) -> Vec<panel::ThreadView> {
        self.threads
            .iter()
            .map(|lt| panel::ThreadView {
                name: lt.thread.name.clone(),
                notes: lt.thread.notes.clone(),
                entries: lt
                    .thread
                    .entries
                    .iter()
                    .map(|e| panel::ThreadEntryView {
                        verse: e.vref.ref_key(),
                        display: e.vref.display(),
                        text: e.text.clone(),
                        note: e.note.clone(),
                    })
                    .collect(),
            })
            .collect()
    }
    fn tags(&self) -> Vec<panel::TagView> {
        self.tags
            .iter()
            .map(|lt| panel::TagView {
                name: lt.tag.name.clone(),
                members: lt
                    .tag
                    .members
                    .iter()
                    .map(|m| match &m.target {
                        TagTarget::Verse(v) => panel::TagMemberView {
                            kind: "verse".into(),
                            verse: Some(v.ref_key()),
                            display: Some(v.display()),
                            strongs: None,
                            note: m.note.clone(),
                        },
                        TagTarget::Concept(c) => panel::TagMemberView {
                            kind: "concept".into(),
                            verse: None,
                            display: None,
                            strongs: Some(c.clone()),
                            note: m.note.clone(),
                        },
                    })
                    .collect(),
            })
            .collect()
    }
    fn weaves(&self) -> Vec<panel::WeaveView> {
        self.weaves
            .iter()
            .enumerate()
            .map(|(index, lw)| panel::WeaveView {
                index,
                name: lw.weave.name.clone(),
                kind_label: lw.weave.kind.label().to_string(),
                notes: lw.weave.notes.clone(),
                suggested: weave::is_suggested(lw),
                links: lw
                    .weave
                    .links
                    .iter()
                    .map(|l| panel::WeaveLinkView {
                        a: l.a.ref_key(),
                        a_display: l.a.display(),
                        b: l.b.ref_key(),
                        b_display: l.b.display(),
                        label: l.label.clone(),
                        span_a: l.span_a.map(|(lo, hi)| [lo, hi]),
                        span_b: l.span_b.map(|(lo, hi)| [lo, hi]),
                    })
                    .collect(),
            })
            .collect()
    }
    fn suggested(&self) -> Vec<panel::SuggestedView> {
        self.weaves
            .iter()
            .filter(|lw| weave::is_suggested(lw))
            .enumerate()
            .map(|(index, lw)| panel::SuggestedView {
                index,
                name: lw.weave.name.clone(),
                kind: lw.weave.kind.token().to_string(),
                notes: lw.weave.notes.clone(),
                lib_index: self.weaves.iter().position(|x| weave::is_suggested(x) && x.weave.name == lw.weave.name),
                links: lw
                    .weave
                    .links
                    .iter()
                    .map(|l| panel::SuggestedLinkView {
                        a: l.a.ref_key(),
                        a_display: l.a.display(),
                        b: l.b.ref_key(),
                        b_display: l.b.display(),
                        label: l.label.clone(),
                    })
                    .collect(),
            })
            .collect()
    }
    fn verse_tokens(&self, refkey: &str) -> Option<panel::VerseTokensView> {
        let v = VRef::parse_ref_key(refkey)?;
        let verse = self.corpus.verse(&v)?;
        Some(panel::VerseTokensView {
            tokens: verse
                .tokens
                .iter()
                .map(|t| panel::TokenView { render: t.render(), added: t.has_flag(corpus::FLAG_ADDED) })
                .collect(),
        })
    }
    fn verse_body(&self, refkey: &str) -> Option<String> {
        let v = VRef::parse_ref_key(refkey)?;
        self.corpus.verse(&v).map(|verse| verse.body())
    }
    fn search(&self, query: &str) -> panel::SearchView {
        match pure_core::search::run_search(&self.corpus, &self.notes, &self.search_ix, query) {
            Some(pure_core::search::SearchAnswer::GoTo { book, chapter, verse }) => {
                let display = match verse {
                    Some(v) => VRef::new(book.clone(), chapter, v).display(),
                    None => format!("{} {}", canon::display_name(&book), chapter),
                };
                panel::SearchView::Goto { book, chapter: chapter as u32, verse: verse.map(u32::from), display }
            }
            Some(pure_core::search::SearchAnswer::Hits { how, total, hits }) => panel::SearchView::Hits {
                capped: total > hits.len(),
                how,
                total,
                hits: hits
                    .into_iter()
                    .map(|h| panel::SearchHitView { verse: h.vref.ref_key(), display: h.vref.display(), note: h.note, why: h.why })
                    .collect(),
            },
            None => panel::SearchView::Hits { how: String::new(), total: 0, capped: false, hits: Vec::new() },
        }
    }
}

/// How many concordance verses a panel card lists before an "… N more" tail.
const PANEL_OCC_CAP: usize = 300;

thread_local! {
    /// The palette the panel-markup helpers colour with (Tier 0 #5). GTK is
    /// single-threaded (one main loop), so a thread-local lets the study panel's
    /// accents follow the theme without threading a palette arg through every
    /// `*_markup` wrapper. Set at load and on each theme switch.
    static MARKUP_PALETTE: RefCell<theme::Palette> = RefCell::new(theme::palette(theme::Theme::Light));
}

/// Point the panel-markup helpers at `palette` (call on load + theme change).
fn set_markup_palette(palette: &theme::Palette) {
    MARKUP_PALETTE.with(|p| *p.borrow_mut() = palette.clone());
}

/// A semantic colour role → the active theme's Pango hex; `None` (Ink) inherits
/// the label's themed ink. Every shell maps these identically, so the panel
/// reads the same on each platform.
fn role_hex(c: panel::Color) -> Option<String> {
    MARKUP_PALETTE.with(|p| p.borrow().panel_color(c).map(str::to_string))
}

/// One styled run as Pango markup: size (logical points → Pango units) + bold/
/// italic, then either an `<a href>` (theme-coloured link) or a role foreground.
fn run_markup(r: &panel::Run) -> String {
    let mut inner = format!("<span size=\"{}\">{}</span>", (r.size * 1024.0).round() as i32, esc(&r.text));
    if r.bold {
        inner = format!("<b>{inner}</b>");
    }
    if r.italic {
        inner = format!("<i>{inner}</i>");
    }
    match &r.uri {
        Some(u) => format!("<a href=\"{}\">{}</a>", esc(u), inner),
        None => match role_hex(r.color) {
            Some(hex) => format!("<span foreground=\"{hex}\">{inner}</span>"),
            None => inner,
        },
    }
}

/// Render the core's typed block list as one Pango markup string for the panel.
fn blocks_to_markup(blocks: &[panel::Block]) -> String {
    let (rule_hex, section_hex) =
        MARKUP_PALETTE.with(|p| (p.borrow().rule.clone(), p.borrow().section.clone()));
    let mut s = String::new();
    for b in blocks {
        match b {
            panel::Block::Rule => {
                s.push_str(&format!("<span foreground=\"{rule_hex}\">──────────────</span>\n"));
            }
            panel::Block::Section { title, mark } => {
                s.push_str(&format!(
                    "\n<span foreground=\"{section_hex}\" size=\"x-small\"><b>{}</b></span>",
                    esc(title)
                ));
                if let Some((glyph, color)) = mark {
                    s.push_str(&format!(
                        " <span foreground=\"{}\" size=\"x-small\">{}</span>",
                        role_hex(*color).unwrap_or_else(|| "#888".to_string()),
                        esc(glyph)
                    ));
                }
                s.push('\n');
            }
            panel::Block::Para { runs, .. } => {
                for r in runs {
                    s.push_str(&run_markup(r));
                }
                s.push('\n');
            }
        }
    }
    s
}

fn word_study_markup(st: &State, hit: &Hit) -> String {
    blocks_to_markup(&panel::word_study(st, st.mode.is_full(), &hit.verse.ref_key(), hit.token_index, &hit.strongs))
}

/// The study of one Strong's code: its dictionary entry and — in Full study —
/// the rendering lens plus the analytics tiers below it. Rendered inline for
/// each of a tapped word's codes, and standalone as the `code:CODE[:word]`
/// card the reverse rendering-lens links open, so an "'love' also translates
/// G5368" link lands on G5368's own entry instead of a bare concordance.
/// `word` is the English surface that led here — its rendering is highlighted
/// and the reverse line stays keyed to it; pass "" when there is none.
fn code_study_markup(st: &State, code: &str, word: &str) -> String {
    blocks_to_markup(&panel::code_study_card(st, st.mode.is_full(), code, word))
}


/// The list of threads, each a link that opens its passages.
/// The whole weave library, flat: name -> compare card (the constellation is
/// the graphical view of the same list).
fn weaves_list_markup(st: &State) -> String {
    blocks_to_markup(&panel::weaves_list(st))
}

fn threads_list_markup(st: &State) -> String {
    blocks_to_markup(&panel::threads_list(st))
}

/// One thread: its passages as jump links with a snapshot preview + note.
fn thread_markup(st: &State, i: usize) -> String {
    blocks_to_markup(&panel::thread_detail(st, i))
}

/// The list of tags, each a link that opens its members.
fn tags_list_markup(st: &State) -> String {
    blocks_to_markup(&panel::tags_list(st))
}

/// One tag: its members — verses as jump links, concepts as concordance links.
fn tag_markup(st: &State, i: usize) -> String {
    blocks_to_markup(&panel::tag_detail(st, i))
}

/// The suggested weaves awaiting review, each with its links and
/// approve/reject actions. Weaves are addressed by their index in `st.weaves`
/// (the flat list of canonical + suggested), so the action links stay valid
/// until the next reload.
fn suggested_list_markup(st: &State) -> String {
    blocks_to_markup(&panel::suggested(st))
}


/// The weave compare card: the weave's kind + notes, then each link as its two
/// linked passages one above the other, the linked words emphasized, with jump
/// links and an "✎ note" editor. `i` is the global index into `st.weaves`.
fn weave_markup(st: &State, i: usize) -> String {
    blocks_to_markup(&panel::compare_card(st, st.mode.is_full(), i))
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


/// Render a list of `(strongs, score)` concept neighbours as concordance links,
/// each shown as `CODE (lemma)` when a lemma is known, comma-separated.
/// How many of a code's occurrences to sample when learning its English gloss.
/// The dominant KJV rendering stabilises well within this; bounding it keeps the
/// radial map cheap to redraw.
const GLOSS_SAMPLE: usize = 80;

/// Trim a KJV token down to its bare word: strip leading/trailing punctuation,
/// keep internal apostrophes/hyphens ("loved," → "loved", "(God" → "God").
fn normalise_word(w: &str) -> String {
    w.trim_matches(|c: char| !c.is_alphanumeric()).to_string()
}

/// A short English gloss for a Strong's code, learned from how the KJV *actually
/// renders it* — the most common English word carrying that code across its
/// occurrences. This is what an English reader recognises ("world" for κόσμος),
/// as opposed to Strong's etymological headword ("orderly arrangement"). Falls
/// back to distilling the dictionary definition when the code is untagged in the
/// corpus. Deterministic: ties break on the lexicographically-first surface form
/// so the same code always glosses the same way between the panel and the map.
fn english_gloss(st: &State, code: &str) -> Option<String> {
    let mut tally: HashMap<String, u32> = HashMap::new();
    for r in st.occ_ix.verses(code).iter().take(GLOSS_SAMPLE) {
        if let Some(v) = st.corpus.verse(r) {
            for t in &v.tokens {
                // Skip translator-supplied words: they render nothing original.
                if t.flags & FLAG_ADDED == 0 && t.strongs.iter().any(|c| c == code) {
                    let w = normalise_word(&t.word);
                    if !w.is_empty() {
                        *tally.entry(w).or_default() += 1;
                    }
                }
            }
        }
    }
    if !tally.is_empty() {
        let mut ranked: Vec<(String, u32)> = tally.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        return Some(ranked[0].0.clone());
    }
    // No tagged occurrence — distil the dictionary as a last resort.
    let e = st.strongs.get(code)?;
    e.def
        .as_deref()
        .and_then(distil_gloss)
        .or_else(|| e.kjv.as_deref().and_then(distil_gloss))
}

/// Distil the first clean English fragment from a Strong's definition/KJV field:
/// drop parenthetical asides, take the leading comma/semicolon-delimited clause,
/// cap its length. A fallback only — the corpus mode above is preferred.
fn distil_gloss(raw: &str) -> Option<String> {
    let mut cleaned = String::with_capacity(raw.len());
    let mut depth: i32 = 0;
    for ch in raw.chars() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = (depth - 1).max(0),
            _ if depth == 0 => cleaned.push(ch),
            _ => {}
        }
    }
    let first = cleaned
        .split(|c| c == ',' || c == ';')
        .map(str::trim)
        .find(|p| p.chars().any(|c| c.is_alphabetic()))?;
    let capped: String = first.chars().take(30).collect();
    // Trim edge punctuation the clause-split leaves behind (a trailing "." from
    // "love.", a leading space from a stripped parenthetical) — keep internal
    // spaces/hyphens ("to love", "self-Existent").
    let g = capped.trim_matches(|c: char| !c.is_alphanumeric()).to_string();
    if g.is_empty() {
        None
    } else {
        Some(g)
    }
}

/// A readable name for a bridge witness source key ("lxx" → "Septuagint").
/// Delegates to the ported `sourceLabel` table so GTK and WinUI agree.
fn humanize_source(key: &str) -> String {
    bridge::source_label(key).to_string()
}


fn search_markup(st: &State, query: &str) -> String {
    blocks_to_markup(&panel::search(st, query))
}

/// Markup for a Strong's code's concordance: every verse it tags, as jump links.
fn concordance_markup(st: &State, code: &str) -> String {
    blocks_to_markup(&panel::concordance(st, code))
}

/// Markup for one rendering of a code's concordance: the verses where the code
/// is translated exactly this way (reached from a RENDERINGS chip), capped at
/// OCC_SHOWN. The passed `rendering` is normalized before lookup, so a chip's
/// display label round-trips through the link unchanged.
fn rendering_concordance_markup(st: &State, code: &str, rendering: &str) -> String {
    blocks_to_markup(&panel::rendering_concordance(st, code, rendering))
}

/// The app CSS, built from the active palette so the scripture paper, pane-nav
/// strip, and active-pane accent follow the theme (Tier 0 #5).
fn css_string(p: &theme::Palette) -> String {
    format!(
        ".scripture {{ background: {paper}; }} \
         label {{ font-family: \"EB Garamond\", serif; }} \
         .studypanel {{ font-size: 14pt; }} \
         .panenav {{ background: {nav}; padding: 3px; }} \
         box.pane-active {{ border-top: 2px solid {gold}; }}",
        paper = p.paper,
        nav = p.pane_nav_bg,
        gold = p.gold,
    )
}

/// Install the app CSS for `palette` and return the provider so a theme switch
/// can re-load it live (`provider.load_from_data(&css_string(&new_palette))`).
fn install_css(palette: &theme::Palette) -> gtk::CssProvider {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(&css_string(palette));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    provider
}

/// Point the icon theme at the bundled hicolor tree and make the woven-cross
/// icon (installed under `APP_ID`) the default for the app's windows, so the
/// title bar / taskbar / alt-tab show it. Mirrors `register_bundled_fonts`:
/// the assets are found by the compile-time manifest path (no system install),
/// which is why this is CI-validated rather than tested on the ARM64 box.
fn install_app_icon() {
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::IconTheme::for_display(&display)
            .add_search_path(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icons"));
    }
    gtk::Window::set_default_icon_name(APP_ID);
}

fn present_error(app: &adw::Application, msg: &str) {
    let data_dir = home::data_dir()
        .map(|d| d.display().to_string())
        .unwrap_or_else(|| "the app data directory".to_string());
    let label = gtk::Label::new(Some(&format!(
        "Could not load scripture data.\n\n{msg}\n\npure-study looks for a data/ folder \
         (kjv.jsonl, strongs.json) in, in order:\n  • $PURE_STUDY_HOME or $OVERLAY_HOME\n  \
         • the current working directory\n  • next to the executable\n  • {data_dir}\n\n\
         Point it at a hydrated tree, e.g.\n  OVERLAY_HOME=../overlay cargo run -p pure-desktop"
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

#[cfg(test)]
mod markup_tests {
    use super::*;

    /// GtkLabel consumes `<a href>` links itself before handing the rest to
    /// Pango, so `pango::parse_markup` doesn't know the `<a>` tag. Strip the
    /// link tags (keeping their text) to validate the Pango-level structure.
    fn strip_links(m: &str) -> String {
        let mut out = String::new();
        let mut rest = m;
        while let Some(i) = rest.find("<a ") {
            out.push_str(&rest[..i]);
            match rest[i..].find('>') {
                Some(j) => rest = &rest[i + j + 1..],
                None => break,
            }
        }
        out.push_str(rest);
        out.replace("</a>", "")
    }

    /// The block→Pango renderer's output must parse as valid Pango markup — the
    /// same check GtkLabel runs on the non-link parts. Exercises section headers
    /// (with a tier mark), links, per-run roles/sizes, a rule, an indented para,
    /// and XML-escaping of `&`/`<`/`>` in run text and link URIs.
    #[test]
    fn blocks_to_markup_is_valid_pango() {
        use pure_core::panel::{Block, Color, Run};
        let blocks = vec![
            Block::Section { title: "SAME ROOT ACROSS TESTAMENTS".into(), mark: None },
            Block::Section { title: "SIMILAR CONCEPTS".into(), mark: Some(("≈".into(), Color::TierMachine)) },
            Block::Para {
                runs: vec![
                    Run::new("create", 13.5, Color::Gold).link("occ:H1254"),
                    Run::new("  etymology + Septuagint", 11.5, Color::Faded),
                    Run::new(" ✝", 11.0, Color::TierGod),
                    Run::new(" a & b <x>", 12.0, Color::Ink),
                ],
                indent: false,
                top_gap: false,
            },
            Block::Rule,
            Block::Para { runs: vec![Run::new("indented", 12.5, Color::Ink)], indent: true, top_gap: true },
        ];
        let markup = blocks_to_markup(&blocks);
        let stripped = strip_links(&markup);
        pango::parse_markup(&stripped, '\0')
            .unwrap_or_else(|e| panic!("invalid panel markup: {e}\n---\n{stripped}"));
        // XML-escaping (not raw &/<>), and the link URI is carried verbatim.
        assert!(markup.contains("&amp;") && markup.contains("&lt;x&gt;"));
        assert!(markup.contains("href=\"occ:H1254\""));
    }

    #[test]
    fn distil_gloss_takes_the_clean_leading_clause() {
        // Parenthetical asides dropped, first clause kept.
        assert_eq!(distil_gloss("(be-)love(-ed)."), Some("love".to_string()));
        assert_eq!(distil_gloss("to love (in a social or moral sense)"), Some("to love".to_string()));
        assert_eq!(distil_gloss("love, i.e. affection or benevolence"), Some("love".to_string()));
        assert_eq!(distil_gloss("()"), None);
    }
}
