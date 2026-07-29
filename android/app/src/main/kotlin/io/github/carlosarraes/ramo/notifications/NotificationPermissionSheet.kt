package io.github.carlosarraes.ramo.notifications

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NotificationPermissionSheet(onEnable: () -> Unit, onNotNow: () -> Unit) {
    ModalBottomSheet(onDismissRequest = onNotNow) {
        Column(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text("Stay ahead of review requests", style = MaterialTheme.typography.titleLarge)
            Text(
                "Ramo can send a quiet notification when a pull request needs your review. No marketing or activity noise.",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Row(
                modifier = Modifier.fillMaxWidth().padding(bottom = 20.dp),
                horizontalArrangement = Arrangement.End,
            ) {
                TextButton(onClick = onNotNow) { Text("Not now") }
                Button(onClick = onEnable) { Text("Enable notifications") }
            }
        }
    }
}
