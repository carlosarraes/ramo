package io.github.carlosarraes.ramo.review

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

@Composable
fun ConversationCard(thread: ReviewThreadUi) {
    Surface(color = MaterialTheme.colorScheme.surfaceVariant, modifier = Modifier.fillMaxWidth().padding(8.dp)) {
        Column(Modifier.padding(12.dp)) {
            Text(if (thread.resolved) "Resolved conversation" else "Conversation", style = MaterialTheme.typography.labelLarge)
            thread.comments.forEach { comment ->
                Text("@${comment.author} · ${comment.createdAt.take(10)}", style = MaterialTheme.typography.labelSmall)
                Text(comment.body)
            }
            if (thread.outdated) Text("Outdated", color = MaterialTheme.colorScheme.tertiary)
        }
    }
}
