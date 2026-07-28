package io.github.carlosarraes.ramo.auth

import android.content.Intent
import android.content.ClipboardManager
import android.content.Context
import android.net.Uri
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp

@Composable
fun TokenScreen(
    state: AuthState,
    onValidate: (String) -> Unit,
    onRetry: () -> Unit,
    onSignOut: () -> Unit,
) {
    var token by remember { mutableStateOf("") }
    val context = LocalContext.current
    val retainedFailure = (state as? AuthState.Failure)?.takeIf { it.tokenRetained }
    Column(
        modifier = Modifier.fillMaxSize().padding(horizontal = 24.dp, vertical = 36.dp),
        verticalArrangement = Arrangement.Center,
    ) {
        Text("ramo", style = MaterialTheme.typography.headlineLarge)
        Spacer(Modifier.height(8.dp))
        Text("A quiet place to review pull requests.", color = MaterialTheme.colorScheme.onSurfaceVariant)
        Spacer(Modifier.height(28.dp))
        if (retainedFailure != null) {
            Text(retainedFailure.failure.message, color = MaterialTheme.colorScheme.error)
            Spacer(Modifier.height(16.dp))
            Button(
                onClick = onRetry,
                modifier = Modifier.fillMaxWidth(),
            ) { Text("Retry") }
            TextButton(
                onClick = onSignOut,
                modifier = Modifier.fillMaxWidth(),
            ) { Text("Sign out") }
            return@Column
        }
        OutlinedTextField(
            value = token,
            onValueChange = { token = it },
            label = { Text("Fine-grained GitHub token") },
            visualTransformation = PasswordVisualTransformation(),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        TextButton(
            onClick = {
                val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                token = clipboard.primaryClip?.getItemAt(0)?.coerceToText(context)?.toString().orEmpty()
            },
        ) { Text("Paste token") }
        Button(
            onClick = { onValidate(token) },
            enabled = state != AuthState.Validating,
            modifier = Modifier.fillMaxWidth(),
        ) { Text(if (state == AuthState.Validating) "Validating…" else "Validate and continue") }
        val failureMessage = when (state) {
            is AuthState.Error -> state.message
            is AuthState.Failure -> state.failure.message
            else -> null
        }
        if (failureMessage != null) {
            Spacer(Modifier.height(12.dp))
            Text(failureMessage, color = MaterialTheme.colorScheme.error)
        }
        Spacer(Modifier.height(16.dp))
        Text(
            "Grant Pull requests: read/write. For team requests, choose the organization as the token's resource owner; GitHub exposes no extra permission for that endpoint.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        TextButton(
            onClick = {
                context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/settings/personal-access-tokens/new")))
            },
        ) { Text("Create a fine-grained token") }
    }
}
