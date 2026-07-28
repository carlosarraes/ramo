package io.github.carlosarraes.ramo.review

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

@Composable
fun NeedsAttentionSheet(state: ReviewUiState, onDelete: (String) -> Unit, onRefresh: () -> Unit) {
    Column(Modifier.fillMaxWidth().padding(12.dp)) {
        Text("The PR changed. Ramo did not move or publish these drafts.")
        state.drafts.forEach { draft ->
            Row(Modifier.fillMaxWidth()) {
                Column(Modifier.weight(1f)) {
                    Text("${draft.path} ${draft.label}")
                    Text(draft.body)
                }
                TextButton(onClick = { onDelete(draft.id) }) { Text("Delete") }
            }
        }
        Button(onClick = onRefresh, enabled = state.drafts.isEmpty()) { Text("Refresh PR") }
    }
}
