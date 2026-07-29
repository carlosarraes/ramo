package io.github.carlosarraes.ramo.review

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DraftEditor(
    editor: DraftEditorUi,
    onSave: (String) -> Unit,
    onCancel: () -> Unit,
) {
    var body by remember(editor.rowKey) { mutableStateOf("") }
    ModalBottomSheet(onDismissRequest = onCancel) {
        Column(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text("Comment on ${editor.label}", style = MaterialTheme.typography.titleLarge)
            OutlinedTextField(
                value = body,
                onValueChange = { body = it },
                label = { Text("Draft comment") },
                minLines = 5,
                singleLine = false,
                modifier = Modifier.fillMaxWidth(),
            )
            Text(
                "Enter adds a new line. Only Save draft finishes this comment.",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodySmall,
            )
            Row(
                modifier = Modifier.fillMaxWidth().padding(bottom = 20.dp),
                horizontalArrangement = Arrangement.End,
            ) {
                TextButton(onClick = onCancel) { Text("Cancel") }
                Button(onClick = { onSave(body) }) { Text("Save draft") }
            }
        }
    }
}
