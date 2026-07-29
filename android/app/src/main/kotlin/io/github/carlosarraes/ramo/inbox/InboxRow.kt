package io.github.carlosarraes.ramo.inbox

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.carlosarraes.ramo.ui.theme.Green
import io.github.carlosarraes.ramo.ui.theme.Red
import java.time.Instant

internal fun relativeAge(now: Long, updated: Long): String {
    val elapsed = (now - updated).coerceAtLeast(0L)
    val minutes = elapsed / 60_000L
    val hours = elapsed / 3_600_000L
    val days = elapsed / 86_400_000L
    return when {
        days > 0 -> "${days}d"
        hours > 0 -> "${hours}h"
        minutes > 0 -> "${minutes}m"
        else -> "now"
    }
}

private fun InboxItem.updatedLabel(nowMillis: Long): String = runCatching {
    relativeAge(nowMillis, Instant.parse(updatedAt).toEpochMilli())
}.getOrElse { updatedAt.take(10) }

@Composable
fun InboxRow(
    item: InboxItem,
    nowMillis: Long,
    reviewRequested: Boolean,
    onOpen: (InboxItem) -> Unit,
) {
    Column(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 88.dp)
            .clickable { onOpen(item) }
            .testTag("inbox-row-${item.nodeId}")
            .semantics { contentDescription = "${item.repository} #${item.number}, ${item.title}" }
            .padding(horizontal = 18.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            Text(
                "${item.repository}  #${item.number}",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.labelMedium,
            )
            Text(
                item.updatedLabel(nowMillis),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.labelMedium,
            )
        }
        Text(
            item.title,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            fontWeight = FontWeight.SemiBold,
            style = MaterialTheme.typography.bodyLarge,
        )
        Row(verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
            Text(
                if (item.draft) "Draft" else if (reviewRequested) "Review requested" else "Open",
                color = MaterialTheme.colorScheme.primary,
                style = MaterialTheme.typography.labelMedium,
            )
            Spacer(Modifier.width(12.dp))
            Text(item.author, color = MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.labelMedium)
            Spacer(Modifier.width(12.dp))
            Text("${item.changedFiles} files", style = MaterialTheme.typography.labelMedium)
            Spacer(Modifier.weight(1f))
            Text("+${item.additions}", color = Green, style = MaterialTheme.typography.labelMedium)
            Spacer(Modifier.width(10.dp))
            Text("−${item.deletions}", color = Red, style = MaterialTheme.typography.labelMedium)
        }
    }
}
