package io.github.carlosarraes.ramo.review

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FinishReviewSheet(
    state: ReviewUiState,
    onDismiss: () -> Unit,
    onBody: (String) -> Unit,
    onVerdict: (ReviewVerdictUi) -> Unit,
    onDeleteDraft: (String) -> Unit,
    onContinue: () -> Unit,
) {
    val pull = state.pullRequest ?: return
    val selfAuthored = pull.author.equals(pull.viewer, ignoreCase = true)
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(Modifier.fillMaxWidth().padding(20.dp)) {
            Text("Finish review · ${state.drafts.size} inline comments")
            state.drafts.forEach { draft ->
                Row(Modifier.fillMaxWidth()) {
                    Text("${draft.path} ${draft.label}", Modifier.weight(1f))
                    TextButton(onClick = { onDeleteDraft(draft.id) }) { Text("Delete") }
                }
            }
            OutlinedTextField(
                value = state.overallBody,
                onValueChange = onBody,
                label = { Text("Overall comment (optional)") },
                minLines = 3,
                modifier = Modifier.fillMaxWidth(),
            )
            VerdictRow("Comment", ReviewVerdictUi.Comment, state.verdict, onVerdict)
            if (!selfAuthored) {
                VerdictRow("Approve", ReviewVerdictUi.Approve, state.verdict, onVerdict)
                VerdictRow("Request changes", ReviewVerdictUi.RequestChanges, state.verdict, onVerdict)
            }
            if (state.needsAttention) Text("The PR changed. Review every draft before publishing.")
            Button(
                onClick = onContinue,
                enabled = !state.publishing && !state.needsAttention && (state.drafts.isNotEmpty() || state.overallBody.isNotBlank()),
                modifier = Modifier.fillMaxWidth(),
            ) { Text("Review and publish") }
        }
    }
}

@Composable
private fun VerdictRow(label: String, value: ReviewVerdictUi, selected: ReviewVerdictUi, onSelect: (ReviewVerdictUi) -> Unit) {
    Row {
        RadioButton(selected = value == selected, onClick = { onSelect(value) })
        TextButton(onClick = { onSelect(value) }) { Text(label) }
    }
}
