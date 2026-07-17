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

use pure_core::config::{self, Config, StudyMode};
use pure_core::corpus::{Corpus, FLAG_ADDED, FLAG_DIVINE, FLAG_TITLE};
use pure_core::search::{self, Notes, SearchAnswer, SearchIx};
use pure_core::strongs::{self, OccurrenceIx, StrongsDict};
use pure_core::renderings::Renderings;
use pure_core::reference::{CANON_SEGMENTS, OT_NT_DIVIDE};
use pure_core::tag::{self, LoadedTag, TagTarget};
use pure_core::thread::{self, LoadedThread};
use pure_core::weave::{self, Link, LoadedWeave, Span, WeaveKind};
use pure_core::{canon, corpus, crossref, home, notes, VRef};
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
        panes,
        active,
    })
}

fn build_ui(app: &adw::Application) {
    // The reader is a warm-paper light design (cream scripture, gold accents);
    // force the light color scheme so the chrome — header nav, dropdowns, the
    // study panel — matches it instead of following a dark system theme (which
    // left light text on the light nav strip, illegible).
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);

    let (cfg, first_run) = config::load();
    let state = match load_state(&cfg) {
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
    let weaves_btn = gtk::Button::with_label("Weaves");
    weaves_btn.set_tooltip_text(Some("Browse the weave library"));
    tags_btn.add_css_class("flat");
    let suggested_btn = gtk::Button::with_label("Suggested");
    suggested_btn.add_css_class("flat");
    suggested_btn.set_tooltip_text(Some("Review proposed weaves — approve to keep, reject to discard"));
    let link_btn = gtk::Button::with_label("＋ link");
    link_btn.add_css_class("flat");
    link_btn.set_tooltip_text(Some("Weave the two pinned words (click a word in each pane; click another word in the same verse to widen the span)"));
    link_btn.set_sensitive(false);
    let map_btn = gtk::Button::with_label("Map");
    map_btn.add_css_class("flat");
    map_btn.set_tooltip_text(Some("Weave map — how strongly each pair of books is woven together"));
    let const_btn = gtk::Button::with_label("Constellation");
    const_btn.add_css_class("flat");
    const_btn.set_tooltip_text(Some(
        "Weave constellation — every weave a labelled lane on the canon backbone; page through the library, pin lanes to compare",
    ));

    // The study tools live together so "Simple reader" mode can hide the whole
    // group at once (decision #4), leaving a clean reader + search + lookup.
    let study_tools = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    study_tools.append(&threads_btn);
    study_tools.append(&tags_btn);
    study_tools.append(&weaves_btn);
    study_tools.append(&suggested_btn);
    study_tools.append(&map_btn);
    study_tools.append(&const_btn);
    study_tools.append(&link_btn);
    header.pack_start(&study_tools);

    // Mode toggle: switch between Simple reader and Full study (persisted).
    let mode_btn = gtk::Button::new();
    mode_btn.add_css_class("flat");
    mode_btn.set_tooltip_text(Some("Switch between Simple reader and Full study"));
    header.pack_end(&mode_btn);

    // Verse-per-line toggle (persisted; the label shows the current mode).
    let vpl_btn = gtk::Button::new();
    vpl_btn.add_css_class("flat");
    vpl_btn.set_tooltip_text(Some("Toggle verse-per-line reading"));
    vpl_btn.set_label(if state.borrow().verse_per_line { "verse / line" } else { "flowing text" });
    header.pack_end(&vpl_btn);

    // Apply a mode to the header chrome: show/hide the study tools + relabel.
    let apply_mode: Rc<dyn Fn(StudyMode)> = {
        let study_tools = study_tools.clone();
        let mode_btn = mode_btn.clone();
        Rc::new(move |mode: StudyMode| {
            study_tools.set_visible(mode.is_full());
            mode_btn.set_label(if mode.is_full() { "Full study" } else { "Simple reader" });
        })
    };
    apply_mode(state.borrow().mode);

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
        weaves_btn.connect_clicked(move |_| {
            let m = weaves_list_markup(&state.borrow());
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
    {
        let state = state.clone();
        let ui = ui.clone();
        map_btn.connect_clicked(move |_| show_weave_map(&state, &ui));
    }
    {
        let state = state.clone();
        let ui = ui.clone();
        const_btn.connect_clicked(move |_| show_constellation(&state, &ui));
    }
    // ── study mode toggle (Simple ⇄ Full), persisted ────────────────────────────
    {
        let state = state.clone();
        let ui = ui.clone();
        let apply_mode = apply_mode.clone();
        mode_btn.connect_clicked(move |_| {
            let new = {
                let mut st = state.borrow_mut();
                st.mode = if st.mode.is_full() { StudyMode::Simple } else { StudyMode::Full };
                st.mode
            };
            persist_config(&state);
            apply_mode(new);
            // Leaving Full: collapse the study panel so no authoring view lingers.
            if !new.is_full() {
                hide_study(&ui);
            }
        });
    }

    // ── verse-per-line toggle, persisted ────────────────────────────────────────
    {
        let state = state.clone();
        let ui = ui.clone();
        let btn = vpl_btn.clone();
        vpl_btn.connect_clicked(move |_| {
            let now = {
                let mut st = state.borrow_mut();
                st.verse_per_line = !st.verse_per_line;
                st.verse_per_line
            };
            persist_config(&state);
            btn.set_label(if now { "verse / line" } else { "flowing text" });
            redraw_all(&ui);
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

    // Save the reading session (open panes + mode + size) on close, so the app
    // reopens where you left it.
    {
        let state = state.clone();
        window.connect_close_request(move |_| {
            persist_config(&state);
            glib::Propagation::Proceed
        });
    }

    install_css();
    install_app_icon();
    rebuild_panes(&state, &ui); // builds the pane columns + first paint
    window.present();

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

    // Neighbours: embedding near (if built) then collocation community.
    let mut nbrs: Vec<(String, bool)> = Vec::new(); // (code, is_semantic)
    if let Some(emb) = &st.embedding {
        for (c, _) in emb.nearest_concepts(code, 6) {
            nbrs.push((c, true));
        }
    }
    if let Some(ce) = &st.concept {
        for c in ce.community(code).into_iter().take(6) {
            if !nbrs.iter().any(|(x, _)| x == &c) {
                nbrs.push((c, false));
            }
        }
    }

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
    close_on_defocus(&win);
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
    close_on_defocus(&win);
    win.present();
}

// ── constellation ───────────────────────────────────────────────────────────
// Ported from overlay `Constellation.hs` + its UI.hs wiring: a scoped overview
// of the weave library, one weave per labelled lane (largest first), nodes on
// the canon book backbone, links as gentle curves. Pinned lanes stay put while
// paging cycles the free lanes past them; click a node to jump, an edge to open
// the weave, the gutter marker to pin.

const CONST_LANES: usize = 18;
const CONST_TOP_PAD: f64 = 18.0;
const CONST_GUTTER: f64 = 150.0;
/// Left edge of the plot: past the gutter plus a margin so a frac-0 node
/// (Genesis 1:1) sits clear of the pin gutter and can't be swallowed by a pin
/// click.
const CONST_PLOT_LEFT: f64 = CONST_GUTTER + 12.0;

/// Transient view state: the page of free lanes, the pinned weave files, and
/// the hovered node. Dropped when the window closes (overlay kept these
/// transient too — `amConstPage`/`amConstPins`).
struct ConstState {
    page: usize,
    pins: Vec<std::path::PathBuf>,
    hover: Option<(usize, VRef)>,
}

/// One lane on the current page: the weave's global index (for the compare
/// card), display name, file identity (for pinning), pin state, and its
/// resolvable links.
struct ConstLane {
    weave_ix: usize,
    name: String,
    file: std::path::PathBuf,
    pinned: bool,
    links: Vec<(VRef, VRef)>,
}

/// Everything the constellation renderer/hit-tester derives from state: the
/// lanes shown on this page (pinned first), plus the paging arithmetic for the
/// caption. Recomputed per event — the weave library is small.
struct ConstPage {
    lanes: Vec<ConstLane>,
    n_pins: usize,
    free_total: usize,
    page: usize,
}

fn const_page(st: &State, cs: &ConstState) -> ConstPage {
    // Usable weaves: at least one link with both ends in this corpus.
    let mut usable: Vec<(usize, Vec<(VRef, VRef)>)> = st
        .weaves
        .iter()
        .enumerate()
        .map(|(i, lw)| {
            let links: Vec<(VRef, VRef)> = lw
                .weave
                .links
                .iter()
                .filter(|l| st.corpus.verse(&l.a).is_some() && st.corpus.verse(&l.b).is_some())
                .map(|l| (l.a.clone(), l.b.clone()))
                .collect();
            (i, links)
        })
        .filter(|(_, ls)| !ls.is_empty())
        .collect();
    // Largest first (stable, so ties keep load order).
    usable.sort_by_key(|(_, ls)| std::cmp::Reverse(ls.len()));

    let pinned_ix: Vec<bool> =
        usable.iter().map(|(i, _)| cs.pins.contains(&st.weaves[*i].file)).collect();
    let n_pins = pinned_ix.iter().filter(|p| **p).count();
    let free_total = usable.len() - n_pins;
    let free_lanes = CONST_LANES.saturating_sub(n_pins);
    let max_page = if free_lanes == 0 || free_total == 0 {
        0
    } else {
        (free_total - 1) / free_lanes
    };
    let page = cs.page.min(max_page);

    // Pinned lanes first, then this page's slice of the free ones.
    let mut lanes: Vec<ConstLane> = Vec::new();
    let mk = |(i, links): &(usize, Vec<(VRef, VRef)>), pinned: bool| ConstLane {
        weave_ix: *i,
        name: st.weaves[*i].weave.name.clone(),
        file: st.weaves[*i].file.clone(),
        pinned,
        links: links.clone(),
    };
    for (u, p) in usable.iter().zip(&pinned_ix) {
        if *p {
            lanes.push(mk(u, true));
        }
    }
    if free_lanes > 0 {
        for (u, _) in usable
            .iter()
            .zip(&pinned_ix)
            .filter(|(_, p)| !**p)
            .skip(page * free_lanes)
            .take(free_lanes)
        {
            lanes.push(mk(u, false));
        }
    }
    ConstPage { lanes, n_pins, free_total, page }
}

/// A verse's canon fraction 0..1: book position plus chapter progress within
/// the book, over the 66 — the same backbone the canon strip uses, so the
/// section labels line up.
fn book_frac(st: &State, r: &VRef) -> f64 {
    let bi = canon::book_order(&r.book).unwrap_or(0) as f64;
    let nc = st.corpus.chapter_count(&r.book).max(1) as f64;
    (bi + (r.chapter.saturating_sub(1)) as f64 / nc) / canon::BOOKS.len() as f64
}

fn const_x(w: f64, frac: f64) -> f64 {
    CONST_PLOT_LEFT + frac.clamp(0.0, 1.0) * (w - CONST_PLOT_LEFT)
}

/// A node's y: its lane's centre plus a small deterministic jitter from the
/// verse identity, so co-lane nodes don't fuse into one flat line.
fn const_node_y(h: f64, n_lanes: usize, lane: usize, r: &VRef) -> f64 {
    let lane_h = (h - CONST_TOP_PAD) / n_lanes.max(1) as f64;
    let hj = ((r.chapter as i64 * 3 + r.verse as i64) % 7 - 3) as f64;
    CONST_TOP_PAD + (lane as f64 + 0.5) * lane_h + hj * lane_h * 0.12
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

/// Per-lane deduped nodes with their witness degree (how many weave links touch
/// the verse across the whole library — node sizing).
fn const_nodes(pg: &ConstPage, deg: &HashMap<VRef, usize>) -> Vec<(usize, VRef, usize)> {
    let mut out = Vec::new();
    for (lane, l) in pg.lanes.iter().enumerate() {
        let mut seen: HashSet<VRef> = HashSet::new();
        for (a, b) in &l.links {
            for r in [a, b] {
                if seen.insert(r.clone()) {
                    out.push((lane, r.clone(), deg.get(r).copied().unwrap_or(0)));
                }
            }
        }
    }
    out
}

/// Witness degree over the whole library: how many weave links touch each verse.
fn const_degrees(st: &State) -> HashMap<VRef, usize> {
    let mut deg: HashMap<VRef, usize> = HashMap::new();
    for lw in &st.weaves {
        for l in &lw.weave.links {
            *deg.entry(l.a.clone()).or_default() += 1;
            *deg.entry(l.b.clone()).or_default() += 1;
        }
    }
    deg
}

fn const_node_radius(deg: usize, max_deg: usize) -> f64 {
    1.4 + 2.4 * (deg as f64 / max_deg.max(1) as f64)
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

/// The node nearest `p` within its radius + 4px slop, tie-broken by distance.
fn const_nearest_node(
    st: &State,
    pg: &ConstPage,
    deg: &HashMap<VRef, usize>,
    w: f64,
    h: f64,
    p: (f64, f64),
) -> Option<(usize, VRef)> {
    let nodes = const_nodes(pg, deg);
    let max_deg = nodes.iter().map(|(_, _, d)| *d).max().unwrap_or(1);
    let n_lanes = pg.lanes.len().max(1);
    nodes
        .into_iter()
        .filter_map(|(lane, r, d)| {
            let x = const_x(w, book_frac(st, &r));
            let y = const_node_y(h, n_lanes, lane, &r);
            let dist = ((p.0 - x).powi(2) + (p.1 - y).powi(2)).sqrt();
            (dist <= const_node_radius(d, max_deg) + 4.0).then_some((lane, r, dist))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(lane, r, _)| (lane, r))
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
    let pg = const_page(&st, &cs);
    let deg = const_degrees(&st);
    let nodes = const_nodes(&pg, &deg);
    let max_deg = nodes.iter().map(|(_, _, d)| *d).max().unwrap_or(1);
    let n_lanes = pg.lanes.len().max(1);
    let lane_h = (hf - CONST_TOP_PAD) / n_lanes as f64;

    // warm-paper backdrop, like the reader
    cr.set_source_rgb(0.988, 0.976, 0.957);
    let _ = cr.rectangle(0.0, 0.0, wf, hf);
    let _ = cr.fill();

    let layout = pangocairo::functions::create_layout(cr);
    let mut fd = pango::FontDescription::new();
    fd.set_family(&st.family);

    // per lane: alternating band, pin marker, weave name
    for (lane, l) in pg.lanes.iter().enumerate() {
        let top = CONST_TOP_PAD + lane as f64 * lane_h;
        let mid = top + lane_h / 2.0;
        if lane % 2 == 0 {
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.03);
            let _ = cr.rectangle(0.0, top, wf, lane_h);
            let _ = cr.fill();
        }
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
    for (lane, l) in pg.lanes.iter().enumerate() {
        let (r, g, b) = const_rgb(lane);
        cr.set_source_rgba(r, g, b, 0.5);
        cr.set_line_width(1.0);
        for (a, bb) in &l.links {
            let (x1, y1) = (const_x(wf, book_frac(&st, a)), const_node_y(hf, n_lanes, lane, a));
            let (x2, y2) =
                (const_x(wf, book_frac(&st, bb)), const_node_y(hf, n_lanes, lane, bb));
            let ddx = x2 - x1;
            cr.move_to(x1, y1);
            cr.curve_to(x1 + ddx * 0.4, y1, x2 - ddx * 0.4, y2, x2, y2);
            let _ = cr.stroke();
        }
    }

    // nodes, sized by witness degree
    for (lane, r, d) in &nodes {
        let (cr_, cg, cb) = const_rgb(*lane);
        let x = const_x(wf, book_frac(&st, r));
        let y = const_node_y(hf, n_lanes, *lane, r);
        let rr = const_node_radius(*d, max_deg);
        cr.set_source_rgb(cr_, cg, cb);
        let _ = cr.rectangle(x - rr, y - rr, 2.0 * rr, 2.0 * rr);
        let _ = cr.fill();
    }

    // hover tooltip: verse · weave
    if let Some((lane, r)) = &cs.hover {
        if let Some(l) = pg.lanes.get(*lane) {
            let x = const_x(wf, book_frac(&st, r));
            let y = const_node_y(hf, n_lanes, *lane, r);
            let txt = format!("{} · {}", r.display(), l.name);
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

/// The paging caption: pins, the honest free-weave range, and the pin hint.
fn const_caption(pg: &ConstPage) -> String {
    let free_lanes = CONST_LANES.saturating_sub(pg.n_pins);
    let range = if pg.free_total == 0 {
        "no free weaves".to_string()
    } else if free_lanes == 0 {
        "all lanes pinned — unpin one to page".to_string()
    } else {
        format!(
            "weaves {}–{} of {}",
            pg.page * free_lanes + 1,
            (pg.free_total).min((pg.page + 1) * free_lanes),
            pg.free_total
        )
    };
    let pins = if pg.n_pins > 0 { format!("{} pinned · ", pg.n_pins) } else { String::new() };
    format!("{pins}{range} · largest first · click the ▪ to pin a lane")
}

/// Open the constellation: one page of weaves as labelled lanes over the canon
/// backbone. ‹/› (or Left/Right) page the free lanes past the pinned ones;
/// click a node to jump the active pane there, an edge to open its weave card,
/// the gutter marker to pin/unpin. Esc or clicking outside closes.
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

    // One refresher shared by every handler: clamp the page, redraw, recaption.
    let refresh: Rc<dyn Fn()> = {
        let (state, cs, area, caption) = (state.clone(), cs.clone(), area.clone(), caption.clone());
        Rc::new(move || {
            let pg = const_page(&state.borrow(), &cs.borrow());
            cs.borrow_mut().page = pg.page;
            caption.set_text(&const_caption(&pg));
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
            let st = state.borrow();
            let pg = const_page(&st, &cs.borrow());
            let deg = const_degrees(&st);
            let hover = const_nearest_node(
                &st,
                &pg,
                &deg,
                area2.width() as f64,
                area2.height() as f64,
                (x, y),
            );
            drop(st);
            if cs.borrow().hover.as_ref() != hover.as_ref() {
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
            let action = {
                let st = state.borrow();
                let pg = const_page(&st, &cs.borrow());
                let deg = const_degrees(&st);
                let n_lanes = pg.lanes.len().max(1);
                // a node/edge always wins over the pin gutter, so a Genesis
                // node on the plot edge navigates rather than pinning
                if let Some((_, r)) = const_nearest_node(&st, &pg, &deg, wf, hf, (x, y)) {
                    Some(('n', r.book.clone(), r.chapter, r.verse, 0))
                } else {
                    let edge = pg.lanes.iter().enumerate().find_map(|(lane, l)| {
                        l.links.iter().find_map(|(a, b)| {
                            let pa = (
                                const_x(wf, book_frac(&st, a)),
                                const_node_y(hf, n_lanes, lane, a),
                            );
                            let pb = (
                                const_x(wf, book_frac(&st, b)),
                                const_node_y(hf, n_lanes, lane, b),
                            );
                            (curve_dist((x, y), pa, pb) <= 5.0).then_some(l.weave_ix)
                        })
                    });
                    match edge {
                        Some(wi) => Some(('e', String::new(), 0, 0, wi)),
                        None => {
                            // pin gutter: left of the plot, on a lane
                            let lane_h = (hf - CONST_TOP_PAD) / n_lanes as f64;
                            let lane = ((y - CONST_TOP_PAD) / lane_h).floor();
                            if x < CONST_GUTTER && lane >= 0.0 && (lane as usize) < pg.lanes.len()
                            {
                                Some(('p', String::new(), 0, 0, lane as usize))
                            } else {
                                None
                            }
                        }
                    }
                }
            };
            match action {
                Some(('n', book, ch, v, _)) => {
                    let active = state.borrow().active;
                    navigate_pane(&state, &ui, active, &book, ch, Some(v));
                }
                Some(('e', _, _, _, wi)) => {
                    let m = weave_markup(&state.borrow(), wi);
                    show_study(&ui, &m);
                    win2.close();
                }
                Some(('p', _, _, _, lane)) => {
                    let file = {
                        let st = state.borrow();
                        let pg = const_page(&st, &cs.borrow());
                        pg.lanes.get(lane).map(|l| l.file.clone())
                    };
                    if let Some(f) = file {
                        let mut c = cs.borrow_mut();
                        match c.pins.iter().position(|p| *p == f) {
                            Some(i) => {
                                c.pins.remove(i);
                            }
                            None => c.pins.push(f),
                        }
                        drop(c);
                        refresh();
                    }
                }
                _ => {}
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
    close_on_defocus(&win);
    win.present();
}

/// Close a transient popup when it stops being the active window — clicking
/// outside (back into the reader, or anywhere else) dismisses it, matching how
/// a popover behaves. Only for view-only popups, never editing dialogs.
fn close_on_defocus(win: &gtk::Window) {
    win.connect_is_active_notify(|w| {
        if !w.is_active() {
            w.close();
        }
    });
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
                gdk::Key::Right | gdk::Key::bracketright => { step_pane(&state, &ui, i, 1); stop }
                gdk::Key::Left | gdk::Key::bracketleft => { step_pane(&state, &ui, i, -1); stop }
                gdk::Key::_0 | gdk::Key::KP_0 if ctrl => { zoom(&state, &ui, 0.0); stop }
                gdk::Key::plus | gdk::Key::equal if ctrl => { zoom(&state, &ui, 1.0); stop }
                gdk::Key::minus if ctrl => { zoom(&state, &ui, -1.0); stop }
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
    } else if let Some(rest) = uri.strip_prefix("rend:") {
        // `rend:CODE:rendering` — a distinct verb from `occ:`, which consumes
        // its whole remainder as the code. Split only on the first colon so a
        // multi-word rendering ("suffereth long") survives intact.
        if let Some((code, rendering)) = rest.split_once(':') {
            let markup = rendering_concordance_markup(&state.borrow(), code, rendering);
            show_study(ui, &markup);
        }
    } else if let Some(rest) = uri.strip_prefix("code:") {
        // `code:CODE[:word]` — the reusable single-code study card (the same
        // block a word study renders per code). Reverse rendering-lens links
        // land here so an unfamiliar code opens its actual entry, not a bare
        // verse list. The optional word is the surface that led here.
        let (code, word) = rest.split_once(':').unwrap_or((rest, ""));
        let st = state.borrow();
        let mut markup = code_study_markup(&st, code, word);
        if st.mode.is_full() {
            markup.push_str(&legend_markup());
        }
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
    } else if let Some(code) = uri.strip_prefix("conceptmap:") {
        show_concept_map(state, ui, code);
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

/// Aggregate the loaded weave links into book-pair ribbons (canon-ordered book
/// indices → link count), plus the max count for scaling. Ported from
/// `ChordMap` aggregation.
fn chord_arcs(st: &State) -> (Vec<(usize, usize, u32)>, u32) {
    let mut m: HashMap<(usize, usize), u32> = HashMap::new();
    for (a, b) in &st.links {
        let (Some(ia), Some(ib)) = (canon::book_order(&a.book), canon::book_order(&b.book)) else {
            continue;
        };
        let key = if ia <= ib { (ia, ib) } else { (ib, ia) };
        *m.entry(key).or_insert(0) += 1;
    }
    let max = m.values().copied().max().unwrap_or(1);
    (m.into_iter().map(|((a, b), c)| (a, b, c)).collect(), max)
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
fn word_study_markup(st: &State, hit: &Hit) -> String {
    let word = st
        .corpus
        .verse(&hit.verse)
        .and_then(|v| v.tokens.get(hit.token_index as usize))
        .map(|t| t.word.clone())
        .unwrap_or_default();

    let mut s = format!(
        "<b>{}</b>\n<span size=\"xx-large\">{}</span>\n",
        esc(&hit.verse.display()),
        esc(&word)
    );

    // Morphology of this exact token (Full study, when the sidecar annotates
    // it): the original-language parse behind the English word.
    if st.mode.is_full() {
        if let Some(g) = st.morph.as_ref().and_then(|m| m.gloss(&hit.verse, hit.token_index)) {
            s.push_str(&format!("<small><span foreground=\"#6a5a2a\">{}</span></small>\n", esc(&g)));
        }
    }
    s.push('\n');

    if hit.strongs.is_empty() {
        s.push_str("<i>no Strong’s tag on this word</i>\n");
    }
    for code in &hit.strongs {
        s.push_str(&code_study_markup(st, code, &word));
        s.push('\n');
    }
    verse_study_extras(st, hit, &mut s);
    // The provenance legend, once, at the foot of a Full-study card.
    if st.mode.is_full() && !hit.strongs.is_empty() {
        s.push_str(&legend_markup());
    }
    s
}

/// The study of one Strong's code: its dictionary entry and — in Full study —
/// the rendering lens plus the analytics tiers below it. Rendered inline for
/// each of a tapped word's codes, and standalone as the `code:CODE[:word]`
/// card the reverse rendering-lens links open, so an "'love' also translates
/// G5368" link lands on G5368's own entry instead of a bare concordance.
/// `word` is the English surface that led here — its rendering is highlighted
/// and the reverse line stays keyed to it; pass "" when there is none.
fn code_study_markup(st: &State, code: &str, word: &str) -> String {
    let mut s = String::new();
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
    // Cross-testament links Strong himself recorded (Full study). Each
    // partner opens its concordance, so a theme can be followed across the
    // Hebrew/Greek boundary the numbering otherwise walls off.
    if st.mode.is_full() {
        // ── rendering lens ──────────────────────────────────────────────
        // The other English words this code is translated as (corpus-
        // derived, not R&D), most frequent first; the tapped word's own
        // rendering is bold. A chip filters the concordance to that
        // rendering; the reverse line names other codes the tapped word
        // also stands for (the agape/phileo split hidden behind "love").
        let rends = st.renderings.renderings(code);
        if !rends.is_empty() {
            s.push_str(&shead_marked("RENDERINGS", MARK_HUMAN));
            let wkey = pure_core::renderings::normalize(word);
            let chips: Vec<String> = rends
                .iter()
                .map(|r| {
                    let tapped =
                        !wkey.is_empty() && pure_core::renderings::normalize(r.label) == wkey;
                    let label = if tapped {
                        format!("<b>{}</b>", esc(r.label))
                    } else {
                        esc(r.label)
                    };
                    format!(
                        "<a href=\"rend:{code}:{rend}\">{label}</a> <span foreground=\"#999\" size=\"x-small\">×{count}</span>",
                        code = esc(code),
                        rend = esc(r.label),
                        count = r.count,
                    )
                })
                .collect();
            s.push_str(&chips.join("  ·  "));
            s.push('\n');

            let others: Vec<(&str, usize)> = st
                .renderings
                .word_codes(word)
                .into_iter()
                .filter(|(c, _)| *c != code)
                .collect();
            if !others.is_empty() {
                let links: Vec<String> = others
                    .iter()
                    .map(|(c, _)| {
                        let label = english_gloss(st, c)
                            .map(|g| format!("{c} ({g})"))
                            .unwrap_or_else(|| c.to_string());
                        // Open the code's own study card (not a bare
                        // concordance), carrying the tapped word so its
                        // rendering is highlighted there.
                        format!("<a href=\"code:{}:{}\">{}</a>", esc(c), esc(word), esc(&label))
                    })
                    .collect();
                s.push_str(&format!(
                    "<small><span foreground=\"#6b6862\">“{}” also translates {}</span></small>\n",
                    esc(word),
                    links.join(", ")
                ));
            }
        }

        // ── analytics tier ──────────────────────────────────────────────
        // Everything below is the R&D layer for this code: cognates, near
        // concepts, co-occurrence, dispersion. Each gets its own header so
        // the reader can tell the tiers apart at a glance.

        // The same root across the Hebrew/Greek divide (Strong's etymology,
        // fused with any external witnesses the pack supplies).
        let partners = st.bridge.partners(code);
        if !partners.is_empty() {
            s.push_str(&shead("SAME ROOT ACROSS TESTAMENTS"));
            let chips: Vec<String> = partners
                .iter()
                .take(6)
                .map(|p| {
                    let label = english_gloss(st, &p.code)
                        .or_else(|| st.strongs.get(&p.code).and_then(|e| e.lemma.clone()))
                        .unwrap_or_else(|| p.code.clone());
                    let srcs = p
                        .sources
                        .iter()
                        .map(|k| humanize_source(k))
                        .collect::<Vec<_>>()
                        .join(", ");
                    // The graded text-witness can disbelieve this pair
                    // (keyed Hebrew-first) — the text's usage pushing back.
                    let (heb, grk) = if code.starts_with('G') {
                        (p.code.as_str(), code)
                    } else {
                        (code, p.code.as_str())
                    };
                    let disputed = if st.witness.disbelief(heb, grk).is_some() {
                        " <span foreground=\"#b04a3a\" size=\"x-small\">· disputed by usage</span>"
                    } else {
                        ""
                    };
                    format!(
                        "<a href=\"occ:{c}\">{label}</a> <span foreground=\"#999\" size=\"x-small\">{srcs}</span>{marks}{disputed}",
                        c = esc(&p.code),
                        label = esc(&label),
                        srcs = esc(&srcs),
                        marks = tier_marks(&p.sources),
                    )
                })
                .collect();
            s.push_str(&chips.join("  ·  "));
            s.push('\n');
        }

        // Distributional near-synonyms from the embedding, with the
        // cross-testament neighbours folded in underneath as a marked line.
        if let Some(emb) = &st.embedding {
            let near = emb.nearest_concepts(code, 6);
            let cross = emb.cross_concepts(code, 6);
            if !near.is_empty() || !cross.is_empty() {
                s.push_str(&shead_marked("SIMILAR CONCEPTS", MARK_MACHINE));
                if !near.is_empty() {
                    s.push_str(&concept_links(st, &near));
                    s.push('\n');
                }
                if !cross.is_empty() {
                    s.push_str("<span foreground=\"#999\" size=\"x-small\">across the testaments —</span> ");
                    s.push_str(&concept_links(st, &cross));
                    s.push('\n');
                }
            }
        }

        // Symbolic collocation field: the concept community this code
        // co-occurs with — "what shares its verses", distinct from the
        // distributional near-synonyms above.
        if let Some(ce) = &st.concept {
            let field = ce.community(code);
            if !field.is_empty() {
                let members: Vec<(String, f32)> = field.into_iter().take(8).map(|c| (c, 0.0)).collect();
                s.push_str(&shead_marked("APPEARS ALONGSIDE", MARK_MACHINE));
                s.push_str(&concept_links(st, &members));
                s.push('\n');
            }

            // Where across the canon this concept concentrates (counts, not
            // chapter references — "Genesis ×12").
            let books = ce.top_books(code, 5);
            if !books.is_empty() {
                let (ot, nt) = ce.testament_split(code);
                let list: Vec<String> = books
                    .iter()
                    .map(|(b, c)| format!("{} ×{}", esc(canon::display_name(b)), c))
                    .collect();
                s.push_str(&shead_marked("WHERE IT CONCENTRATES", MARK_MACHINE));
                s.push_str(&format!(
                    "{}  <span foreground=\"#999\" size=\"x-small\">(OT {ot} · NT {nt})</span>\n",
                    list.join(" · ")
                ));
            }
        }

        // Leitwort: a concept the text packs into one stretch far denser
        // than chance (a repeated motif). Show where and how much of it
        // lands there; the score is −log₁₀ of the adjusted tail p-value.
        if let Some(b) = st.leitwort.as_ref().and_then(|m| m.get(code)) {
            let label = burst::span_label(|id| canon::display_name(id).to_string(), &b.win_start, &b.win_end);
            s.push_str(&shead_marked("LEITWORT", MARK_MACHINE));
            s.push_str(&format!(
                "{count} of its {total} uses cluster in {label} <span foreground=\"#999\" size=\"x-small\">(p ≈ 10<sup>−{sig:.0}</sup>)</span>\n",
                count = b.win_count,
                total = b.n,
                label = esc(&label),
                sig = b.score
            ));
        }

        // A link into the graphical concept map (radial neighbourhood +
        // dispersion strip) for this code.
        if st.concept.is_some() {
            s.push_str(&format!("\n<a href=\"conceptmap:{}\"><small>▸ open concept map</small></a>\n", esc(code)));
        }
    }
    s
}

/// The per-verse extras that follow a word's code blocks: author actions,
/// weave + TSK cross-references, "verses like this", tags, and margin notes.
/// Appended to `s`, so it shares the word study's markup buffer.
fn verse_study_extras(st: &State, hit: &Hit, s: &mut String) {
    // Author actions on this verse — only in Full study mode.
    let rk = esc(&hit.verse.ref_key());
    if st.mode.is_full() {
        s.push_str(&format!(
            "\n<a href=\"addtag:{rk}\">＋ tag verse</a>    <a href=\"addthread:{rk}\">＋ add to thread</a>\n"
        ));
    }

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

    // TSK topical cross-references for this verse (Full study), best-voted
    // first — a curated study tier, shown clearly labelled and never blessed
    // into weaves.
    if st.mode.is_full() {
        if let Some(rs) = st.xref_ix.get(&hit.verse) {
            s.push_str(&format!("\n<b>study cross-references ({})</b>  <small><span foreground=\"#888\">TSK</span></small>{}\n", rs.len(), MARK_HUMAN));
            for r in rs.iter().take(XREF_SHOWN) {
                let target = match &r.end {
                    Some(e) => format!("{}–{}", go_link(&r.to), go_link(e)),
                    None => go_link(&r.to),
                };
                s.push_str(&format!("{target}\n"));
            }
            if rs.len() > XREF_SHOWN {
                s.push_str(&format!("<small>… {} more</small>\n", rs.len() - XREF_SHOWN));
            }
        }
    }

    // "Verses like this" from the SIF embedding model (Full study): thematic
    // neighbours by concept, distinct from the curated/weave links above.
    if st.mode.is_full() {
        if let Some(vs) = &st.verse_sim {
            let sim = vs.similar_verses_in(&hit.verse, 6);
            if !sim.is_empty() {
                s.push_str(&format!("\n<b>verses like this</b>  <small><span foreground=\"#888\">by concept</span></small>{}\n", MARK_MACHINE));
                for (r, _) in &sim {
                    s.push_str(&format!("{}\n", go_link(r)));
                }
            }
            let cross = vs.similar_verses_cross(&hit.verse, 4);
            if !cross.is_empty() {
                s.push_str("<small><span foreground=\"#9e7d38\">across the testaments: </span></small>\n");
                for (r, _) in &cross {
                    s.push_str(&format!("{}\n", go_link(r)));
                }
            }
        }
    }

    // Tags that include this verse (Full study only).
    if st.mode.is_full() {
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
    }

    // The 1769 translators' margin notes on this verse, if any.
    if let Some(ns) = st.notes.get(&hit.verse) {
        s.push_str(&format!("\n<b>margin notes</b>{}\n", MARK_HUMAN));
        for n in ns {
            s.push_str(&format!("<small>{}</small>\n", esc(n)));
        }
    }
}

/// The list of threads, each a link that opens its passages.
/// The whole weave library, flat: name -> compare card (the constellation is
/// the graphical view of the same list).
fn weaves_list_markup(st: &State) -> String {
    if st.weaves.is_empty() {
        return "<b>Weaves</b>\n\n<i>None loaded.</i>".to_string();
    }
    let mut order: Vec<usize> = (0..st.weaves.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(st.weaves[i].weave.links.len()));
    let mut s = format!("<b>Weaves ({})</b>\n\n", st.weaves.len());
    for i in order {
        let lw = &st.weaves[i];
        let n = lw.weave.links.len();
        s.push_str(&format!(
            "<a href=\"weave:{}\">{}</a>  <small>{} \u{b7} {} link{}{}</small>\n",
            i,
            esc(&lw.weave.name),
            esc(lw.weave.kind.label()),
            n,
            if n == 1 { "" } else { "s" },
            if weave::is_suggested(lw) { " \u{b7} suggested" } else { "" }
        ));
    }
    s
}

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
    let edit = if st.mode.is_full() {
        format!("  <a href=\"editweavenotes:{i}\"><small>✎ note</small></a>")
    } else {
        String::new()
    };
    s.push_str(&format!(
        "<small>{} link{}</small>{edit}\n",
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

// ── authority-tier provenance marks (overlay `Tier` / `provIcon`) ───────────
// Every piece of evidence carries a small glyph saying where it comes from, so
// the reader always knows the provenance. A single GtkLabel's Pango markup
// can't embed images (as overlay's Monomer panels do), so we use overlay's
// glyph-fallback set, colored per tier: God = the text itself, Human = curated
// scholarship, Machine = a learned/aligned artifact (weigh, don't trust); a
// flask flags a method that is still research-grade (a lead, not a result).
const MARK_GOD: &str = " <span foreground=\"#9e7d38\" size=\"x-small\">✝</span>";
const MARK_HUMAN: &str = " <span foreground=\"#6f8f6a\" size=\"x-small\">†</span>";
const MARK_MACHINE: &str = " <span foreground=\"#999\" size=\"x-small\">≈</span>";
const MARK_RESEARCH: &str = " <span foreground=\"#b04a3a\" size=\"x-small\">⚗</span>";

/// The provenance marks for one evidence item: the deduped union of its witness
/// sources' tiers (additive — a multi-source item shows every tier it attests,
/// never one "winning" tier) plus a flask when any source's method is still
/// research-grade. A Human-only chip still gets the dagger; a scripture
/// quotation shows ✝ ≈ ⚗ together.
fn tier_marks(sources: &[String]) -> String {
    let mut s = String::new();
    for t in bridge::tiers_of(sources) {
        s.push_str(match t {
            bridge::Tier::God => MARK_GOD,
            bridge::Tier::Human => MARK_HUMAN,
            bridge::Tier::Machine => MARK_MACHINE,
        });
    }
    if sources.iter().any(|k| bridge::research_grade(k)) {
        s.push_str(MARK_RESEARCH);
    }
    s
}

/// A small-caps-feel section header for the study panel: a spaced, muted-gold
/// label on its own line, with a blank line above it, and an optional trailing
/// tier mark for blocks whose whole provenance is one tier (RENDERINGS = Human,
/// the analytics = Machine). Titles are given already upper-cased.
fn shead_marked(title: &str, mark: &str) -> String {
    format!(
        "\n<span foreground=\"#a0894a\" size=\"x-small\"><b>{}</b></span>{}\n",
        title, mark
    )
}

/// The common case: a section header with no tier mark.
fn shead(title: &str) -> String {
    shead_marked(title, "")
}

/// The provenance legend, shown once at the foot of a Full-study card so the
/// marks are learnable. Overlay conveys each mark's meaning through a hover
/// tooltip; a single GtkLabel can't, so we spell the key out.
fn legend_markup() -> String {
    concat!(
        "\n<span size=\"x-small\" foreground=\"#999\">where this comes from:  ",
        "<span foreground=\"#9e7d38\">✝</span> the text  ·  ",
        "<span foreground=\"#6f8f6a\">†</span> curated scholarship  ·  ",
        "<span foreground=\"#999\">≈</span> machine-derived, weigh it  ·  ",
        "<span foreground=\"#b04a3a\">⚗</span> research-grade</span>\n"
    )
    .to_string()
}

/// Render concept neighbours English-first: the recognisable English gloss is
/// the link, with the original-language lemma trailing dimmed ("love ἀγάπη").
/// Members are separated by middots so the two-part chips stay legible.
fn concept_links(st: &State, items: &[(String, f32)]) -> String {
    items
        .iter()
        .map(|(code, _)| {
            let english = english_gloss(st, code);
            let lemma = st.strongs.get(code).and_then(|e| e.lemma.clone());
            match (english, lemma) {
                // English word links to the concordance; lemma trails, dimmed.
                (Some(en), Some(lem)) => format!(
                    "<a href=\"occ:{c}\">{en}</a> <span foreground=\"#8a7a52\" size=\"small\">{lem}</span>",
                    c = esc(code),
                    en = esc(&en),
                    lem = esc(&lem)
                ),
                (Some(en), None) => format!("<a href=\"occ:{c}\">{en}</a>", c = esc(code), en = esc(&en)),
                (None, Some(lem)) => format!("<a href=\"occ:{c}\">{lem}</a>", c = esc(code), lem = esc(&lem)),
                (None, None) => format!("<a href=\"occ:{c}\">{c}</a>", c = esc(code)),
            }
        })
        .collect::<Vec<_>>()
        .join("  ·  ")
}

/// Markup for a search query's answer: a go-to jump, or a list of hit links.
/// One dim context line under a search hit: the verse text windowed around
/// the first (ASCII-case-insensitive) match of the query\u{2019}s first word, the
/// match bolded. Char-boundary safe; empty when the verse text is empty.
fn search_snippet_markup(body: &str, query: &str) -> String {
    if body.is_empty() {
        return String::new();
    }
    fn snap(s: &str, mut i: usize) -> usize {
        while i < s.len() && !s.is_char_boundary(i) {
            i += 1;
        }
        i.min(s.len())
    }
    fn find_ascii_ci(hay: &str, needle: &str) -> Option<usize> {
        let (h, n) = (hay.as_bytes(), needle.as_bytes());
        if n.is_empty() || n.len() > h.len() {
            return None;
        }
        (0..=h.len() - n.len())
            .filter(|&i| hay.is_char_boundary(i))
            .find(|&i| (0..n.len()).all(|j| h[i + j].eq_ignore_ascii_case(&n[j])))
    }
    let needle = query.split_whitespace().next().unwrap_or("");
    let hit = find_ascii_ci(body, needle);
    const WINDOW: usize = 46;
    let (start, end) = match hit {
        Some(at) => (
            snap(body, at.saturating_sub(WINDOW)),
            snap(body, (at + needle.len() + WINDOW).min(body.len())),
        ),
        None => (0, snap(body, (2 * WINDOW).min(body.len()))),
    };
    // Snap outward to word boundaries.
    let start = if start > 0 { body[..start].rfind(' ').map_or(start, |i| i + 1) } else { 0 };
    let end = body[end..].find(' ').map_or(body.len(), |i| end + i);
    let pre = if start > 0 { "\u{2026}" } else { "" };
    let post = if end < body.len() { "\u{2026}" } else { "" };
    let mid = match hit {
        Some(at) if at >= start && at + needle.len() <= end => format!(
            "{}<b>{}</b>{}",
            esc(&body[start..at]),
            esc(&body[at..at + needle.len()]),
            esc(&body[at + needle.len()..end])
        ),
        _ => esc(&body[start..end]),
    };
    format!("<small><span foreground=\"#8a8378\">{pre}{mid}{post}</span></small>\n")
}

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
                if let Some(v) = st.corpus.verse(&h.vref) {
                    s.push_str(&search_snippet_markup(&v.body(), query));
                }
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

/// Markup for one rendering of a code's concordance: the verses where the code
/// is translated exactly this way (reached from a RENDERINGS chip), capped at
/// OCC_SHOWN. The passed `rendering` is normalized before lookup, so a chip's
/// display label round-trips through the link unchanged.
fn rendering_concordance_markup(st: &State, code: &str, rendering: &str) -> String {
    let key = pure_core::renderings::normalize(rendering);
    let occs = st.renderings.rendering_occs(code, rendering);
    // Recover the display label (most-common surface) for the header.
    let label = st
        .renderings
        .renderings(code)
        .into_iter()
        .find(|r| pure_core::renderings::normalize(r.label) == key)
        .map(|r| r.label.to_string())
        .unwrap_or_else(|| rendering.to_string());
    let mut s = format!(
        "<b>{}</b>  <span size=\"large\">“{}”</span>\n<small>{} verse{} rendered “{}”</small>\n\n",
        esc(code),
        esc(&label),
        occs.len(),
        if occs.len() == 1 { "" } else { "s" },
        esc(&label),
    );
    for o in occs.iter().take(OCC_SHOWN) {
        s.push_str(&format!("{}\n", go_link(&o.vref)));
    }
    if occs.len() > OCC_SHOWN {
        s.push_str(&format!("\n<small>… {} more</small>", occs.len() - OCC_SHOWN));
    }
    s
}

fn install_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        ".scripture { background: #fcf9f4; } \
         label { font-family: \"EB Garamond\", serif; } \
         .studypanel { font-size: 14pt; } \
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

    /// Every study-panel markup shape must parse as valid Pango markup — the
    /// same check GtkLabel runs on the non-link parts. Guards the section
    /// headers, the English-first concept chips, the humanised bridge chips,
    /// the `<sup>` exponent in the leitwort line, and the authority-tier
    /// provenance marks (per-chip + on headers) plus the legend against a
    /// syntax regression.
    #[test]
    fn panel_markup_shapes_are_valid_pango() {
        let samples = [
            // A whole analytics run, headers and all, concatenated as the label
            // would receive it — with tier marks on headers, per-chip
            // provenance marks on the bridge partner, and the legend at the end.
            format!(
                "{h1}<a href=\"occ:H1254\">create</a> <span foreground=\"#999\" size=\"x-small\">etymology, Septuagint</span>{marks} <span foreground=\"#b04a3a\" size=\"x-small\">· disputed by usage</span>\n\
                 {h2}<a href=\"occ:G25\">love</a> <span foreground=\"#8a7a52\" size=\"small\">ἀγαπάω</span>  ·  <a href=\"occ:G5368\">love</a> <span foreground=\"#8a7a52\" size=\"small\">φιλέω</span>\n\
                 <span foreground=\"#999\" size=\"x-small\">across the testaments —</span> <a href=\"occ:H2617\">mercy</a> <span foreground=\"#8a7a52\" size=\"small\">חֶסֶד</span>\n\
                 {h3}Genesis ×12 · Psalms ×9  <span foreground=\"#999\" size=\"x-small\">(OT 3 · NT 2)</span>\n\
                 {h4}42 of its 55 uses cluster in Genesis 1–11 <span foreground=\"#999\" size=\"x-small\">(p ≈ 10<sup>−43</sup>)</span>\n\
                 {legend}",
                h1 = shead("SAME ROOT ACROSS TESTAMENTS"),
                h2 = shead_marked("SIMILAR CONCEPTS", MARK_MACHINE),
                h3 = shead_marked("WHERE IT CONCENTRATES", MARK_MACHINE),
                h4 = shead_marked("LEITWORT", MARK_MACHINE),
                // quotation (God+Machine, research-grade) fused with etymology
                // (Human) → all four glyphs ✝ † ≈ ⚗.
                marks = tier_marks(&["quotation".to_string(), "etymology".to_string()]),
                legend = legend_markup(),
            ),
            // The RENDERINGS header carries the Human mark.
            shead_marked("RENDERINGS", MARK_HUMAN),
        ];
        for m in &samples {
            let stripped = strip_links(m);
            pango::parse_markup(&stripped, '\0')
                .unwrap_or_else(|e| panic!("invalid panel markup: {e}\n---\n{stripped}"));
        }
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
