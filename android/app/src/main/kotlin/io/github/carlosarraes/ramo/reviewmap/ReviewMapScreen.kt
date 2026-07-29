package io.github.carlosarraes.ramo.reviewmap

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.carlosarraes.ramo.ui.components.FailureBanner

data class ReviewMapCallbacks(
    val onBack: () -> Unit,
    val onOpenFile: (String) -> Unit,
    val onToggleGroup: (String) -> Unit,
    val onRetry: () -> Unit,
    val onDismissFailure: () -> Unit,
)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ReviewMapScreen(state: ReviewMapUiState, callbacks: ReviewMapCallbacks) {
    val map = state.map
    Scaffold(
        modifier = Modifier.fillMaxSize(),
        contentWindowInsets = WindowInsets.safeDrawing,
        topBar = {
            TopAppBar(
                title = { Text("Review Map", fontWeight = FontWeight.SemiBold) },
                navigationIcon = {
                    TextButton(onClick = callbacks.onBack, modifier = Modifier.heightIn(min = 48.dp)) { Text("Back") }
                },
                actions = {
                    map?.let {
                        Text("+${it.additions}", color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Bold)
                        Text(" −${it.deletions}", color = MaterialTheme.colorScheme.error, fontWeight = FontWeight.Bold)
                    }
                },
            )
        },
    ) { padding ->
        if (map == null) {
            Column(Modifier.fillMaxSize().padding(padding).padding(24.dp), verticalArrangement = Arrangement.Center) {
                Text(if (state.loading) "Building exact map…" else "Review Map unavailable")
                state.failure?.let { FailureBanner(it.message, it.retryable, callbacks.onRetry, callbacks.onDismissFailure) }
            }
            return@Scaffold
        }
        LazyColumn(Modifier.fillMaxSize().padding(padding)) {
            item {
                val status = when (state.phase) {
                    ReviewMapPhase.Analyzing -> "Analyzing privately on your laptop…"
                    ReviewMapPhase.Enriched -> "Local analysis · ${map.analysisModel ?: "ready"}"
                    ReviewMapPhase.Unpaired -> "Exact map · pair laptop analysis in Settings"
                    ReviewMapPhase.Offline -> "Exact map · laptop analysis offline"
                    else -> "Exact map"
                }
                Text(status, Modifier.padding(horizontal = 18.dp, vertical = 10.dp), color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            state.failure?.let { failure ->
                item { FailureBanner(failure.message, failure.retryable, callbacks.onRetry, callbacks.onDismissFailure) }
            }
            map.files.minByOrNull { it.recommendedOrder ?: Int.MAX_VALUE }?.let { first ->
                item {
                    Button(
                        onClick = { callbacks.onOpenFile(first.path) },
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 18.dp, vertical = 8.dp).heightIn(min = 48.dp),
                    ) { Text("Start with ${first.path.substringAfterLast('/')}") }
                }
            }
            map.groups.forEach { group ->
                val expanded = group.id in state.expandedGroups
                item(key = group.id) {
                    GroupRow(group, expanded) { callbacks.onToggleGroup(group.id) }
                }
                if (expanded) {
                    items(group.fileIds, key = { it }) { id ->
                        map.fileById[id]?.let { file ->
                            FileRow(file, file.path in state.reviewedPaths, callbacks.onOpenFile)
                        }
                    }
                }
            }
            item { Text("${state.reviewedPaths.size} of ${map.files.size} files viewed", Modifier.padding(18.dp), color = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
    }
}

@Composable
private fun GroupRow(group: ReviewMapGroupUi, expanded: Boolean, onClick: () -> Unit) {
    Column(
        Modifier.fillMaxWidth().clickable(role = Role.Button, onClick = onClick)
            .semantics { stateDescription = if (expanded) "Expanded" else "Collapsed" }
            .padding(horizontal = 18.dp, vertical = 12.dp),
    ) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text(if (expanded) "⌄ " else "› ", color = MaterialTheme.colorScheme.onSurfaceVariant)
            Text("${group.label} · ${group.fileIds.size} files", Modifier.weight(1f), fontWeight = FontWeight.SemiBold)
            ChangeCounts(group.additions, group.deletions)
        }
        group.summary?.let { Text(it, Modifier.padding(start = 20.dp, top = 4.dp), color = MaterialTheme.colorScheme.onSurfaceVariant) }
        group.risk?.let { Text(it, Modifier.padding(start = 20.dp, top = 2.dp), color = MaterialTheme.colorScheme.error) }
    }
    HorizontalDivider()
}

@Composable
private fun FileRow(file: ReviewMapFileUi, reviewed: Boolean, onOpen: (String) -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable { onOpen(file.path) }.heightIn(min = 56.dp)
            .padding(start = 38.dp, end = 18.dp, top = 10.dp, bottom = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(file.path, fontFamily = FontFamily.Monospace, maxLines = 1, overflow = TextOverflow.Ellipsis)
            val detail = listOfNotNull(file.summary, file.owner?.let { "Owner $it" }, if (reviewed || file.viewed) "Viewed" else null).joinToString(" · ")
            if (detail.isNotBlank()) Text(detail, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2)
        }
        ChangeCounts(file.additions, file.deletions)
    }
}

@Composable
private fun ChangeCounts(additions: Long, deletions: Long) {
    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Text("+$additions", color = MaterialTheme.colorScheme.primary, fontFamily = FontFamily.Monospace)
        Text("−$deletions", color = MaterialTheme.colorScheme.error, fontFamily = FontFamily.Monospace)
    }
}
