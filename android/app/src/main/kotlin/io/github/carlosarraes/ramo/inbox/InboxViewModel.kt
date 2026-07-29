package io.github.carlosarraes.ramo.inbox

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import io.github.carlosarraes.ramo.errors.toUserFacingFailure
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

class InboxViewModel(
    private val repository: InboxRepository,
    private val cache: InboxCache,
    private val nowMillis: () -> Long = System::currentTimeMillis,
) : ViewModel() {
    private val mutableState = MutableStateFlow(InboxUiState())
    val state: StateFlow<InboxUiState> = mutableState.asStateFlow()
    private val jobs = mutableMapOf<InboxTab, Job>()

    init {
        cache.load()?.let { cached ->
            val refreshedAt = cached.refreshedAtEpochMillis.takeIf { it > 0 }
            mutableState.value = InboxUiState(
                reviewRequests = cached.reviewRequests.toTabState(true, refreshedAt),
                authored = cached.authored.toTabState(true, refreshedAt),
            )
        }
    }

    fun select(tab: InboxTab) {
        mutableState.value = mutableState.value.copy(selected = tab)
    }

    fun refresh() {
        val tab = mutableState.value.selected
        jobs[tab]?.cancel()
        update(tab) { it.copy(loading = true, failure = null) }
        jobs[tab] = viewModelScope.launch {
            runCatching { repository.load(tab, null) }
                .onSuccess { page ->
                    val refreshedAt = nowMillis()
                    update(tab) { page.toTabState(refreshedAtEpochMillis = refreshedAt) }
                    saveCache()
                }
                .onFailure { error ->
                    val failure = error.toUserFacingFailure("Could not load pull requests")
                    update(tab) { current ->
                        current.copy(
                            loading = false,
                            failure = if (current.items.isEmpty()) failure
                            else failure.copy(message = "Offline · showing last refresh"),
                        )
                    }
                }
        }
    }

    fun loadMore() {
        val tab = mutableState.value.selected
        val current = mutableState.value.tab(tab)
        if (current.loading || !current.hasNextPage) return
        update(tab) { it.copy(loading = true, failure = null) }
        jobs[tab] = viewModelScope.launch {
            runCatching { repository.load(tab, current.cursor) }
                .onSuccess { page ->
                    update(tab) {
                        val merged = (it.items + page.items).distinctBy(InboxItem::nodeId)
                        page.toTabState().copy(
                            items = merged,
                            refreshedAtEpochMillis = it.refreshedAtEpochMillis,
                        )
                    }
                }
                .onFailure { error ->
                    update(tab) {
                        it.copy(
                            loading = false,
                            failure = error.toUserFacingFailure("Could not load more"),
                        )
                    }
                }
        }
    }

    fun clear() = cache.clear()

    fun setQuery(query: String) {
        mutableState.value = mutableState.value.copy(query = query)
    }

    fun visibleItems(): List<InboxItem> {
        val value = mutableState.value
        return value.tab(value.selected).visibleItems(value.query)
    }

    fun dismissFailure(tab: InboxTab) {
        update(tab) { it.copy(failure = null) }
    }

    private fun saveCache() {
        val value = mutableState.value
        cache.save(
            InboxCacheValue(
                reviewRequests = value.reviewRequests.toPage(),
                authored = value.authored.toPage(),
                refreshedAtEpochMillis = listOfNotNull(
                    value.reviewRequests.refreshedAtEpochMillis,
                    value.authored.refreshedAtEpochMillis,
                ).maxOrNull() ?: 0L,
            ),
        )
    }

    private fun update(tab: InboxTab, transform: (TabState) -> TabState) {
        val value = mutableState.value
        mutableState.value = if (tab == InboxTab.ReviewRequests) {
            value.copy(reviewRequests = transform(value.reviewRequests))
        } else {
            value.copy(authored = transform(value.authored))
        }
    }
}

private fun InboxPage.toTabState(
    fromCache: Boolean = false,
    refreshedAtEpochMillis: Long? = null,
) = TabState(
    items,
    cursor,
    hasNextPage,
    fromCache = fromCache,
    warnings = warnings,
    refreshedAtEpochMillis = refreshedAtEpochMillis,
)
private fun TabState.toPage() = InboxPage(items, cursor, hasNextPage, warnings)
