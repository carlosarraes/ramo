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
) : ViewModel() {
    private val mutableState = MutableStateFlow(InboxUiState())
    val state: StateFlow<InboxUiState> = mutableState.asStateFlow()
    private val jobs = mutableMapOf<InboxTab, Job>()

    init {
        cache.load()?.let { (reviews, authored) ->
            mutableState.value = InboxUiState(
                reviewRequests = reviews.toTabState(fromCache = true),
                authored = authored.toTabState(fromCache = true),
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
                    update(tab) { page.toTabState() }
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
                        page.toTabState().copy(items = merged)
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

    private fun saveCache() {
        val value = mutableState.value
        cache.save(value.reviewRequests.toPage(), value.authored.toPage())
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

private fun InboxPage.toTabState(fromCache: Boolean = false) = TabState(
    items, cursor, hasNextPage, fromCache = fromCache, warnings = warnings,
)
private fun TabState.toPage() = InboxPage(items, cursor, hasNextPage, warnings)
