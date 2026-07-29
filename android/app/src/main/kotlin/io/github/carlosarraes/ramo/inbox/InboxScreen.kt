package io.github.carlosarraes.ramo.inbox

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.github.carlosarraes.ramo.ui.components.FailureBanner

internal fun TabState.visibleItems(query: String): List<InboxItem> {
    val normalized = query.trim()
    if (normalized.isEmpty()) return items
    return items.filter { item ->
        listOf(item.repository, item.title, item.author, "#${item.number}")
            .any { field -> field.contains(normalized, ignoreCase = true) }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun InboxScreen(
    login: String,
    state: InboxUiState,
    nowMillis: Long = System.currentTimeMillis(),
    onSelect: (InboxTab) -> Unit,
    onQuery: (String) -> Unit,
    onDismissFailure: (InboxTab) -> Unit,
    onRefresh: () -> Unit,
    onLoadMore: () -> Unit,
    onOpen: (InboxItem) -> Unit,
    onSettings: () -> Unit,
    onSignOut: () -> Unit,
) {
    val tab = state.tab(state.selected)
    var searching by rememberSaveable { mutableStateOf(false) }
    var menuOpen by rememberSaveable { mutableStateOf(false) }
    val visibleItems = tab.visibleItems(state.query)

    Scaffold(
        modifier = Modifier.fillMaxSize(),
        contentWindowInsets = WindowInsets.safeDrawing,
        topBar = {
            TopAppBar(
                title = {
                    if (searching) {
                        TextField(
                            value = state.query,
                            onValueChange = onQuery,
                            placeholder = { Text("Search reviews") },
                            singleLine = true,
                        )
                    } else {
                        Text("Review queue", fontWeight = FontWeight.SemiBold)
                    }
                },
                actions = {
                    IconButton(
                        onClick = {
                            searching = !searching
                            if (!searching) onQuery("")
                        },
                        modifier = Modifier.semantics { contentDescription = "Search" },
                    ) { Text(if (searching) "×" else "⌕") }
                    Box {
                        IconButton(
                            onClick = { menuOpen = true },
                            modifier = Modifier.semantics { contentDescription = "More options" },
                        ) { Text("⋮") }
                        DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                            DropdownMenuItem(
                                text = { Text("Refresh") },
                                onClick = { menuOpen = false; onRefresh() },
                            )
                            DropdownMenuItem(
                                text = { Text("Settings") },
                                onClick = { menuOpen = false; onSettings() },
                            )
                            DropdownMenuItem(text = { Text("@$login") }, onClick = {}, enabled = false)
                            DropdownMenuItem(
                                text = { Text("Sign out") },
                                onClick = { menuOpen = false; onSignOut() },
                            )
                        }
                    }
                },
            )
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                FilterChip(
                    selected = state.selected == InboxTab.ReviewRequests,
                    onClick = { onSelect(InboxTab.ReviewRequests) },
                    label = { Text("Review requests") },
                )
                FilterChip(
                    selected = state.selected == InboxTab.Authored,
                    onClick = { onSelect(InboxTab.Authored) },
                    label = { Text("Your PRs") },
                )
            }
            tab.failure?.takeIf { tab.items.isNotEmpty() }?.let { failure ->
                FailureBanner(
                    message = failure.message,
                    retryable = failure.retryable,
                    onRetry = onRefresh,
                    onDismiss = { onDismissFailure(state.selected) },
                )
            }
            PullToRefreshBox(
                isRefreshing = tab.loading && tab.items.isNotEmpty(),
                onRefresh = onRefresh,
                modifier = Modifier.weight(1f),
            ) {
                when {
                    tab.loading && tab.items.isEmpty() -> CenteredMessage("Loading…")
                    tab.failure != null && tab.items.isEmpty() -> Box(
                        Modifier.fillMaxSize().padding(24.dp),
                        contentAlignment = Alignment.Center,
                    ) {
                        FailureBanner(
                            message = tab.failure.message,
                            retryable = tab.failure.retryable,
                            onRetry = onRefresh,
                            onDismiss = { onDismissFailure(state.selected) },
                        )
                    }
                    visibleItems.isEmpty() -> CenteredMessage(
                        if (state.query.isNotBlank()) {
                            "No matching pull requests"
                        } else if (state.selected == InboxTab.ReviewRequests) {
                            "No reviews waiting"
                        } else {
                            "No open pull requests"
                        },
                    )
                    else -> LazyColumn(Modifier.fillMaxSize()) {
                        tab.warnings.forEach { warning ->
                            item { Text(warning, Modifier.padding(16.dp), color = MaterialTheme.colorScheme.tertiary) }
                        }
                        items(visibleItems, key = InboxItem::nodeId) { item ->
                            InboxRow(item, nowMillis, onOpen)
                            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                        }
                        if (tab.hasNextPage && state.query.isBlank()) {
                            item {
                                TextButton(onClick = onLoadMore, modifier = Modifier.fillMaxWidth()) {
                                    Text("Load more")
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun CenteredMessage(message: String) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Text(message, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}
