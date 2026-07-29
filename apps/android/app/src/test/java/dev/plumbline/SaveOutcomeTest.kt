// A write the engine refused must never look like one it took.
//
// Every authoring endpoint's contract is "null = success, else the reason"
// (StudyEngine). Three Compose surfaces used to drop that answer on the floor and
// close anyway — the verse sheet's note dialog, the notes browser's editor, and the
// memorize drill's grade (v1.0 audit, 2026-07-29) — so a full disk or a refused
// write took the reader's words with the sheet, silently. They all read the answer
// through `saveOutcome` now, and this is where the decision is pinned: Saved closes,
// Failed keeps the surface open carrying the engine's own words.
//
// Pure logic, plain JUnit, no Android runtime.

package dev.plumbline

import dev.plumbline.ui.SaveOutcome
import dev.plumbline.ui.noteSaveFailedLine
import dev.plumbline.ui.saveOutcome
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SaveOutcomeTest {
    /** The ABI's success: a null error string. */
    @Test
    fun nullMeansSaved() {
        assertEquals(SaveOutcome.Saved, saveOutcome(Result.success(null)))
    }

    /** An empty/whitespace error means the same as null — nothing went wrong. */
    @Test
    fun blankErrorMeansSaved() {
        assertEquals(SaveOutcome.Saved, saveOutcome(Result.success("")))
        assertEquals(SaveOutcome.Saved, saveOutcome(Result.success("   ")))
    }

    /** The bug itself: a disk-full write answered with a reason, and the surface
     *  closed as if it had saved. */
    @Test
    fun anEngineErrorIsAFailureCarryingTheReason() {
        val diskFull =
            "i/o error reading /data/user/0/dev.plumbline/files/user/notes/John.3.16.json: " +
                "No space left on device (os error 28)"
        val outcome = saveOutcome(Result.success(diskFull))
        assertTrue(
            "a save the engine refused must not report Saved — the reader's note is gone with the sheet " +
                "(got $outcome)",
            outcome is SaveOutcome.Failed,
        )
        assertEquals(
            "the engine's reason must reach the reader, not be swallowed",
            diskFull,
            (outcome as SaveOutcome.Failed).message,
        )
    }

    /** A bad ref is refused the same way, and must not close the editor either. */
    @Test
    fun aBadRefIsAFailure() {
        val outcome = saveOutcome(Result.success("bad ref: Jhn 3:16"))
        assertTrue(
            "a refused ref must keep the editor open, not report Saved (got $outcome)",
            outcome is SaveOutcome.Failed,
        )
    }

    /** A throw from the native side is a failure too — `runCatching { … }.getOrNull()`
     *  used to flatten it to null, which every call site read as success. */
    @Test
    fun aThrownCallIsAFailureNotASuccess() {
        val outcome = saveOutcome(Result.failure(UnsatisfiedLinkError("libplumbline_ffi.so not loaded")))
        assertTrue(
            "a thrown engine call must not report Saved (got $outcome)",
            outcome is SaveOutcome.Failed,
        )
        assertEquals(
            "libplumbline_ffi.so not loaded",
            (outcome as SaveOutcome.Failed).message,
        )
    }

    /** A throw with no message of its own still gets human copy: a blank reason in
     *  the dialog reads as a glitch, not as "your note is not saved". */
    @Test
    fun aMessagelessThrowStillGetsAReason() {
        val outcome = saveOutcome(Result.failure(RuntimeException()))
        assertTrue(
            "a message-less throw must still be a failure (got $outcome)",
            outcome is SaveOutcome.Failed,
        )
        assertTrue(
            "a failure the reader can see must say something",
            (outcome as SaveOutcome.Failed).message.isNotBlank(),
        )
    }

    /** The note editor's line: the engine's words kept, plus the promise the dialog
     *  keeps by staying open. */
    @Test
    fun theNoteFailureLineKeepsTheReasonAndTheNote() {
        val line = noteSaveFailedLine("No space left on device (os error 28)")
        assertTrue(
            "the reader must be told why it did not save: $line",
            line.contains("No space left on device (os error 28)"),
        )
        assertTrue(
            "the reader must be told their words are still in the dialog: $line",
            line.contains("still here"),
        )
    }
}
