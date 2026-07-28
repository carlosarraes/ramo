package io.github.carlosarraes.ramo.inbox

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.PrimaryTabRow
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.carlosarraes.ramo.ui.theme.Green
import io.github.carlosarraes.ramo.ui.theme.Red

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun InboxScreen(
    login: String,
    state: InboxUiState,
    onSelect: (InboxTab) -> Unit,
    onRefresh: () -> Unit,
    onLoadMore: () -> Unit,
    onOpen: (InboxItem) -> Unit,
    onSignOut: () -> Unit,
) {
    val tab = state.tab(state.selected)
    Column(Modifier.fillMaxSize()) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text("ramo", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
            TextButton(onClick = onSignOut) { Text("@$login · Sign out") }
        }
        PrimaryTabRow(selectedTabIndex = state.selected.ordinal) {
            Tab(
                selected = state.selected == InboxTab.ReviewRequests,
                onClick = { onSelect(InboxTab.ReviewRequests) },
                text = { Text("Review requests") },
            )
            Tab(
                selected = state.selected == InboxTab.Authored,
                onClick = { onSelect(InboxTab.Authored) },
                text = { Text("Your PRs") },
            )
        }
        PullToRefreshBox(isRefreshing = tab.loading && tab.items.isNotEmpty(), onRefresh = onRefresh) {
            when {
                tab.loading && tab.items.isEmpty() -> Text("Loading…", Modifier.padding(20.dp))
                tab.error != null && tab.items.isEmpty() -> Column(Modifier.padding(20.dp)) {
                    Text(tab.error, color = MaterialTheme.colorScheme.error)
                    Button(onClick = onRefresh) { Text("Try again") }
                }
                tab.items.isEmpty() -> Text(
                    if (state.selected == InboxTab.ReviewRequests) "No reviews waiting" else "No open pull requests",
                    Modifier.padding(20.dp),
                )
                else -> LazyColumn(Modifier.fillMaxSize()) {
                    tab.error?.let { message -> item { Text(message, Modifier.padding(12.dp)) } }
                    tab.warnings.forEach { warning -> item { Text(warning, Modifier.padding(12.dp), color = MaterialTheme.colorScheme.tertiary) } }
                    items(tab.items, key = InboxItem::nodeId) { item ->
                        PullRequestRow(item, onOpen)
                        HorizontalDivider()
                    }
                    if (tab.hasNextPage) {
                        item { TextButton(onClick = onLoadMore, modifier = Modifier.fillMaxWidth()) { Text("Load more") } }
                    }
                }
            }
        }
    }
}

@Composable
private fun PullRequestRow(item: InboxItem, onOpen: (InboxItem) -> Unit) {
    Column(
        Modifier
            .fillMaxWidth()
            .clickable { onOpen(item) }
            .semantics { contentDescription = "${item.repository} #${item.number}, ${item.title}" }
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        Text("${item.repository}  #${item.number}", style = MaterialTheme.typography.labelMedium)
        Text(item.title, maxLines = 2, overflow = TextOverflow.Ellipsis, fontWeight = FontWeight.SemiBold)
        Row {
            Text("+${item.additions}", color = Green)
            Spacer(Modifier.width(10.dp))
            Text("−${item.deletions}", color = Red)
            Spacer(Modifier.width(10.dp))
            Text("${item.changedFiles} files")
            Spacer(Modifier.width(10.dp))
            Text(item.updatedAt.take(10), color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}
