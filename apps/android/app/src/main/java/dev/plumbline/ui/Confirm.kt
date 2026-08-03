// The one confirmation, for anything that destroys something.
//
// The web twin is shell/ConfirmDialog.svelte + `session.askConfirm`. Both exist
// because the app had four different answers to "does this ask first?": deleting a
// memorize card asked nothing, rejecting a suggested weave asked nothing,
// untagging asked nothing, and deleting a thread had an AlertDialog built by hand
// at its own call site (2026-07-29). Whether an action asks should be a property of
// the action, not of whoever wrote its button.
//
// The confirm button NAMES THE ACT — "Delete thread", "Remove card", "Reject" —
// rather than saying OK. A reader who only half-read the sentence still knows what
// the button is about to do, and the destructive one is the one that is tinted.
//
// Author D (Compose UI).

package dev.plumbline.ui

import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable

/**
 * A pending destructive action: what to say, what the button is called, and what
 * to do if the reader says yes.
 *
 * [onConfirm] runs on the caller's thread — keep engine work inside it off the
 * main thread exactly as the call site would have anyway.
 */
data class ConfirmRequest(
    val title: String,
    val body: String,
    val verb: String = t("common.delete"),
    val onConfirm: () -> Unit,
)

/**
 * Show [request] if there is one. [onDismiss] clears it — a confirmation the
 * reader cannot back out of is not a confirmation, so the scrim and the system
 * back gesture both mean no (AlertDialog gives both).
 */
@Composable
fun ConfirmDialog(request: ConfirmRequest?, palette: ReaderPalette, onDismiss: () -> Unit) {
    if (request == null) return
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = palette.panelBg,
        title = { Text(request.title, color = palette.ink) },
        text = { Text(request.body, color = palette.faded) },
        confirmButton = {
            TextButton(onClick = {
                onDismiss()
                request.onConfirm()
            }) { Text(request.verb, color = palette.disputed) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(t("common.cancel"), color = palette.faded) }
        },
    )
}
