package io.github.carlosarraes.ramo.review

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier

@Composable
fun DraftEditor(
    editor: DraftEditorUi,
    onExtendPrevious: () -> Unit,
    onExtendNext: () -> Unit,
    onSave: (String) -> Unit,
    onCancel: () -> Unit,
) {
    var body by remember(editor.rowKey) { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onCancel,
        title = { Text("Comment on ${editor.label}") },
        text = {
            Column {
                Row {
                    TextButton(onClick = onExtendPrevious) { Text("Include previous") }
                    TextButton(onClick = onExtendNext) { Text("Include next") }
                }
                OutlinedTextField(
                    value = body,
                    onValueChange = { body = it },
                    label = { Text("Draft comment") },
                    minLines = 4,
                    singleLine = false,
                    modifier = Modifier.fillMaxWidth(),
                )
                Text("Enter adds a new line. Save draft is the only action that finishes editing.")
            }
        },
        confirmButton = { Button(onClick = { onSave(body) }) { Text("Save draft") } },
        dismissButton = { TextButton(onClick = onCancel) { Text("Cancel") } },
    )
}
