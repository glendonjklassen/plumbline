//! Grammatical function words — Strong's codes for articles, conjunctions,
//! prepositions, pronouns, negations, the be-verbs, and bare deictic adverbs.
//!
//! Concept-neighbour surfaces (embedding spokes, collocates) exclude these:
//! function words co-occur with *everything*, so distributionally they sit
//! "near" every content word — the concept map for *believe* was offering
//! *because* as a similar concept (product feedback, 2026-07-25). The query
//! code itself is never filtered — a reader can still open ὅτι or הָיָה
//! directly and study it; the codes below just stop appearing as neighbours.
//!
//! Curated by hand against `data/strongs.json` (each entry eyeballed against
//! its lemma + KJV definition) rather than derived from morphology: the
//! morphology sidecar is an optional pack, and neighbour lists must not change
//! shape depending on which packs happen to be installed.

/// Sorted for binary search — `cargo test` guards the ordering.
const FUNCTION_WORDS: &[&str] = &[
    // ── Greek ──
    "G1063", // γάρ for
    "G1065", // γέ indeed / at least
    "G1161", // δέ but / and
    "G1211", // δή now / doubtless
    "G1223", // διά through / because of
    "G1352", // διό wherefore
    "G1437", // ἐάν if
    "G1438", // ἑαυτοῦ himself / herself
    "G1473", // ἐγώ I
    "G1487", // εἰ if
    "G1488", // εἶ (thou) art
    "G1498", // εἴην should be
    "G1499", // εἰ καί if also / although
    "G1510", // εἰμί I am (still studyable directly)
    "G1511", // εἶναι to be
    "G1519", // εἰς into
    "G1526", // εἰσί are
    "G1537", // ἐκ out of
    "G1563", // ἐκεῖ there
    "G1565", // ἐκεῖνος that (one)
    "G1691", // ἐμέ me
    "G1715", // ἔμπροσθεν before
    "G1722", // ἐν in
    "G1799", // ἐνώπιον before / in the sight of
    "G1909", // ἐπί upon
    "G2070", // ἐσμέν we are
    "G2071", // ἔσομαι shall be
    "G2075", // ἐστέ ye are
    "G2076", // ἐστί is
    "G2077", // ἔστω let it be
    "G2193", // ἕως until
    "G2228", // ἤ or
    "G2248", // ἡμᾶς us
    "G2249", // ἡμεῖς we
    "G2252", // ἤμην I was
    "G2257", // ἡμῶν of us
    "G2258", // ἦν was
    "G2277", // ἤτω let it be
    "G235",  // ἀλλά but
    "G240",  // ἀλλήλων one another
    "G2443", // ἵνα so that
    "G2504", // κἀγώ and I
    "G2531", // καθώς even as
    "G2532", // καί and
    "G2596", // κατά according to
    "G303",  // ἀνά each / apiece
    "G3165", // μέ me
    "G3303", // μέν indeed
    "G3326", // μετά with / after
    "G3360", // μέχρι until
    "G3361", // μή not
    "G3366", // μηδέ neither
    "G3383", // μήτε neither
    "G3427", // μοί to me
    "G3450", // μοῦ of me / my
    "G3568", // νῦν now
    "G3588", // ὁ the
    "G3699", // ὅπου where
    "G3704", // ὅπως so that
    "G3739", // ὅς who / which
    "G3745", // ὅσος as much as
    "G3748", // ὅστις whosoever
    "G3754", // ὅτι that / because
    "G3756", // οὐ not
    "G3761", // οὐδέ neither
    "G3767", // οὖν therefore
    "G3777", // οὔτε neither
    "G3779", // οὕτω thus
    "G3844", // παρά beside / from
    "G3956", // πᾶς all / every
    "G4012", // περί about / concerning
    "G4253", // πρό before
    "G4314", // πρός toward
    "G4571", // σέ thee
    "G4671", // σοί to thee
    "G4675", // σοῦ of thee / thy
    "G473",  // ἀντί instead of
    "G4771", // σύ thou
    "G4862", // σύν with
    "G5037", // τέ also / both
    "G5100", // τὶς some / any
    "G5101", // τίς who? / what?
    "G5119", // τότε then
    "G5209", // ὑμᾶς you
    "G5210", // ὑμεῖς ye
    "G5216", // ὑμῶν of you / your
    "G5228", // ὑπέρ on behalf of / above
    "G5259", // ὑπό by / under
    "G5565", // χωρίς without
    "G5600", // ὦ may be
    "G5602", // ὧδε here
    "G5607", // ὤν being
    "G5613", // ὡς as
    "G5618", // ὥσπερ even as
    "G575",  // ἀπό from
    "G686",  // ἄρα then / therefore
    "G891",  // ἄχρι until
    // ── Hebrew / Aramaic ──
    "H1157", // בְּעַד for / through
    "H1571", // גַּם also
    "H176",  // אוֹ or
    "H1768", // דִּי (Aramaic) that / which
    "H1931", // הוּא he / it / the same
    "H1961", // הָיָה to be / become (copula; still studyable directly)
    "H1992", // הֵם they
    "H2063", // זֹאת this (f.)
    "H2088", // זֶה this
    "H227",  // אָז then
    "H310",  // אַחַר after
    "H3588", // כִּי for / because / that
    "H3605", // כֹּל all / every
    "H3651", // כֵּן so / thus
    "H3808", // לֹא not
    "H408",  // אַל not (jussive)
    "H4100", // מָה what?
    "H413",  // אֵל unto / toward
    "H428",  // אֵלֶּה these
    "H4310", // מִי who?
    "H4480", // מִן from / out of
    "H4616", // מַעַן in order that
    "H518",  // אִם if
    "H5704", // עַד until
    "H587",  // אֲנַחְנוּ we
    "H589",  // אֲנִי I
    "H5921", // עַל upon / over
    "H595",  // אָנֹכִי I
    "H5973", // עִם with
    "H637",  // אַף also / yea
    "H8033", // שָׁם there
    "H834",  // אֲשֶׁר who / which
    "H8478", // תַּחַת under / instead of
    "H853",  // אֵת (object marker, untranslated)
    "H854",  // אֵת with
    "H859",  // אַתָּה thou
    "H996",  // בֵּין between
];

/// Whether a Strong's code is a grammatical function word — a code that
/// concept-neighbour surfaces should skip.
pub fn is_function_word(code: &str) -> bool {
    FUNCTION_WORDS.binary_search(&code).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_is_sorted_for_binary_search() {
        let mut sorted = FUNCTION_WORDS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(FUNCTION_WORDS, &sorted[..], "keep FUNCTION_WORDS sorted + deduped");
    }

    #[test]
    fn known_function_words_match() {
        for c in ["G2532", "G3754", "G1063", "H853", "H3588", "H5921"] {
            assert!(is_function_word(c), "{c} should be a function word");
        }
    }

    #[test]
    fn content_words_pass() {
        // believe, love, God, beginning, create, word
        for c in ["G4100", "G26", "G2316", "H7225", "H1254", "G3056"] {
            assert!(!is_function_word(c), "{c} must not be filtered");
        }
    }
}
