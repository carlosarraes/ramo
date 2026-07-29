package io.github.carlosarraes.ramo.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    login: String,
    codeSize: Int,
    notificationsGranted: Boolean,
    localAnalysisEndpoint: String? = null,
    onCodeSize: (Int) -> Unit,
    onEnableNotifications: () -> Unit,
    onBack: () -> Unit,
    onUnpair: () -> Unit = {},
    onSignOut: () -> Unit,
) {
    var confirmUnpair by remember { mutableStateOf(false) }
    Scaffold(
        contentWindowInsets = WindowInsets.safeDrawing,
        topBar = {
            TopAppBar(
                title = { Text("Settings") },
                navigationIcon = {
                    TextButton(onClick = onBack, modifier = Modifier.heightIn(min = 48.dp)) {
                        Text("Back")
                    }
                },
            )
        },
    ) { contentPadding ->
        Column(
            modifier = Modifier.fillMaxSize().padding(contentPadding).padding(horizontal = 20.dp),
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text("Account", style = MaterialTheme.typography.labelLarge)
                Text("@$login", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
            }
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Code size · $codeSize", style = MaterialTheme.typography.titleMedium)
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    OutlinedButton(onClick = { onCodeSize(codeSize - 1) }, enabled = codeSize > 11) {
                        Text("Smaller")
                    }
                    OutlinedButton(onClick = { onCodeSize(codeSize + 1) }, enabled = codeSize < 20) {
                        Text("Larger")
                    }
                }
            }
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Local analysis", style = MaterialTheme.typography.titleMedium)
                Text(
                    localAnalysisEndpoint?.let { "Paired with ${java.net.URI(it).host}" }
                        ?: "Not paired. Scan the QR code printed by ramo server pair.",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (localAnalysisEndpoint != null) {
                    OutlinedButton(onClick = { confirmUnpair = true }) { Text("Unpair laptop") }
                }
            }
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Notifications", style = MaterialTheme.typography.titleMedium)
                Text(
                    if (notificationsGranted) "Review-request notifications are enabled."
                    else "Get a quiet alert when a review is requested.",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (!notificationsGranted) {
                    Button(onClick = onEnableNotifications) { Text("Enable notifications") }
                }
            }
            Spacer(Modifier.weight(1f))
            TextButton(
                onClick = onSignOut,
                modifier = Modifier.fillMaxWidth().padding(bottom = 12.dp),
            ) {
                Text("Sign out", color = MaterialTheme.colorScheme.error)
            }
        }
    }
    if (confirmUnpair) {
        androidx.compose.material3.AlertDialog(
            onDismissRequest = { confirmUnpair = false },
            title = { Text("Unpair laptop analysis?") },
            text = { Text("Exact Review Maps will still work. This removes the encrypted phone credential.") },
            confirmButton = {
                TextButton(onClick = { confirmUnpair = false; onUnpair() }) { Text("Unpair") }
            },
            dismissButton = { TextButton(onClick = { confirmUnpair = false }) { Text("Cancel") } },
        )
    }
}
