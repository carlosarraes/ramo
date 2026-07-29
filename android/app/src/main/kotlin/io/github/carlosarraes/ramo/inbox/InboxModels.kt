package io.github.carlosarraes.ramo.inbox

import io.github.carlosarraes.ramo.errors.UserFacingFailure

enum class InboxTab { ReviewRequests, Authored }

data class InboxItem(
    val nodeId: String,
    val repository: String,
    val number: Long,
    val title: String,
    val url: String,
    val author: String,
    val updatedAt: String,
    val draft: Boolean,
    val additions: Long,
    val deletions: Long,
    val changedFiles: Long,
)

data class InboxPage(
    val items: List<InboxItem> = emptyList(),
    val cursor: String? = null,
    val hasNextPage: Boolean = false,
    val warnings: List<String> = emptyList(),
)

data class InboxCacheValue(
    val reviewRequests: InboxPage,
    val authored: InboxPage,
    val refreshedAtEpochMillis: Long,
)

data class TabState(
    val items: List<InboxItem> = emptyList(),
    val cursor: String? = null,
    val hasNextPage: Boolean = false,
    val loading: Boolean = false,
    val failure: UserFacingFailure? = null,
    val fromCache: Boolean = false,
    val warnings: List<String> = emptyList(),
    val refreshedAtEpochMillis: Long? = null,
)

data class InboxUiState(
    val selected: InboxTab = InboxTab.ReviewRequests,
    val reviewRequests: TabState = TabState(),
    val authored: TabState = TabState(),
    val query: String = "",
) {
    fun tab(tab: InboxTab) = if (tab == InboxTab.ReviewRequests) reviewRequests else authored
}
