package io.github.carlosarraes.ramo.review

import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable

@Composable
fun PublishConfirmation(state: ReviewUiState, onPublish: () -> Unit, onDismiss: () -> Unit) {
    val pull = state.pullRequest ?: return
    val action = when (state.verdict) {
        ReviewVerdictUi.Comment -> "Comment on"
        ReviewVerdictUi.Approve -> "Approve"
        ReviewVerdictUi.RequestChanges -> "Request changes on"
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Publish review?") },
        text = { Text("$action PR #${pull.number} with ${state.drafts.size} inline comments?") },
        confirmButton = {
            Button(onClick = onPublish, enabled = !state.publishing) {
                Text(if (state.publishing) "Publishing…" else "Publish")
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
