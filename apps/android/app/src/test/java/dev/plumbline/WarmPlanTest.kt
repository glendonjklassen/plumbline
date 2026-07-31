package dev.plumbline

// What a cold start is allowed to build.
//
// The shell used to force every lazy index at every launch, machine tier
// included — and the machine tier has been OFF by default since the tiers went
// opt-in, so a reader who never asked for concept/leitwort analysis paid its
// corpus-wide scans on every single launch, for panels the gates then refuse to
// draw. The decision is [warmPlan]; these tests are the decision, pinned. There
// is nothing on-device to run here: it is a pure function of the reader's two
// settings.

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class WarmPlanTest {

    /** The indexes only a tier's own panels can reach. The bridge is the curated
     *  tier's since 2026-07-30 — `panel.rs` reaches `bridge_partners` under
     *  `gates.human` alone, and the concept map that banded it is gone. */
    private val machineSet = listOf(WarmIndex.Concept, WarmIndex.Leitwort)
    private val humanSet = listOf(WarmIndex.Renderings, WarmIndex.StudyXrefs, WarmIndex.Bridge)

    /** The four reader settings there are. */
    private val allTiers = listOf(false to false, true to false, false to true, true to true)

    @Test
    fun tiersOffWarmsOnlyWhatEveryReaderNeeds() {
        // Search (one tap away, and not analysis) and the occurrence index (every
        // word study prints its count before it consults a single gate). Nothing
        // else: no tier is on, so no tier's panels can be drawn.
        assertEquals(
            listOf(WarmIndex.Search, WarmIndex.Occurrences),
            warmPlan(humanAnalysis = false, machineAnalysis = false),
        )
    }

    @Test
    fun machineTierOffNeverWarmsTheMachineSet() {
        // THE BUG. Every launch built these, off-by-default tier and all.
        for ((human, machine) in allTiers.filter { !it.second }) {
            val plan = warmPlan(humanAnalysis = human, machineAnalysis = machine)
            for (ix in machineSet) {
                assertFalse(
                    "machineAnalysis=false still warms the machine tier's $ix (plan: $plan)",
                    plan.contains(ix),
                )
            }
        }
    }

    @Test
    fun machineOnWarmsTheMachineSet() {
        val plan = warmPlan(humanAnalysis = false, machineAnalysis = true)
        for (ix in machineSet) {
            assertTrue("machineAnalysis=true does not warm $ix (plan: $plan)", plan.contains(ix))
        }
        // …and it is the MACHINE set, not everything: the curated tier is off,
        // and the bridge went with it when the concept map's partner band died.
        for (ix in humanSet) {
            assertFalse("machine-only warm still builds $ix (plan: $plan)", plan.contains(ix))
        }
    }

    @Test
    fun humanOnWarmsTheCuratedSetAndNoAnalytics() {
        val plan = warmPlan(humanAnalysis = true, machineAnalysis = false)
        for (ix in humanSet) {
            assertTrue("humanAnalysis=true does not warm $ix (plan: $plan)", plan.contains(ix))
        }
        for (ix in machineSet) {
            assertFalse("curated-only warm still builds $ix (plan: $plan)", plan.contains(ix))
        }
    }

    @Test
    fun searchIsWarmedEitherWay() {
        for ((human, machine) in allTiers) {
            val plan = warmPlan(humanAnalysis = human, machineAnalysis = machine)
            assertTrue(
                "search is not warmed for human=$human machine=$machine (plan: $plan)",
                plan.contains(WarmIndex.Search),
            )
            // First, deliberately: the search build is the one step that nests a
            // study read guard, and a cold start is the moment no writer exists.
            assertEquals(
                "search is not the first warm step for human=$human machine=$machine",
                WarmIndex.Search,
                plan.first(),
            )
            // The occurrence index is every word tap's, gates or no gates.
            assertTrue(
                "occurrences are not warmed for human=$human machine=$machine (plan: $plan)",
                plan.contains(WarmIndex.Occurrences),
            )
        }
    }

    @Test
    fun bothTiersOnWarmsEveryIndex() {
        // The reader who asked for everything gets exactly what this shell always
        // did — and MainActivity spots that case by size, taking the single
        // `WarmIndexes()` call instead of a probe per index.
        val plan = warmPlan(humanAnalysis = true, machineAnalysis = true)
        assertEquals("both tiers on must cover every index", WarmIndex.entries.size, plan.size)
        for (ix in WarmIndex.entries) {
            assertTrue("both tiers on does not warm $ix (plan: $plan)", plan.contains(ix))
        }
    }

    @Test
    fun noPlanRepeatsAStep() {
        // What makes the size check above sound: a duplicated step could reach the
        // full count without covering every index, and the shell would then take
        // the build-everything path for a reader who asked for one tier.
        for ((human, machine) in allTiers) {
            val plan = warmPlan(humanAnalysis = human, machineAnalysis = machine)
            assertEquals(
                "warm plan repeats a step for human=$human machine=$machine: $plan",
                plan.distinct(),
                plan,
            )
        }
    }
}
