package io.github.carlosarraes.ramo.inbox

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals

@OptIn(ExperimentalCoroutinesApi::class)
class InboxViewModelTest {
    private val dispatcher = StandardTestDispatcher()
    @BeforeTest fun setup() = Dispatchers.setMain(dispatcher)
    @AfterTest fun teardown() = Dispatchers.resetMain()

    @Test fun defaultsToReviewRequestsAndPreservesTabs() = runTest(dispatcher) {
        val repository = FakeInboxRepository()
        val model = InboxViewModel(repository, MemoryInboxCache())
        assertEquals(InboxTab.ReviewRequests, model.state.value.selected)
        model.refresh()
        advanceUntilIdle()
        model.select(InboxTab.Authored)
        model.refresh()
        advanceUntilIdle()
        assertEquals("reviews", model.state.value.reviewRequests.items.single().nodeId)
        assertEquals("authored", model.state.value.authored.items.single().nodeId)
    }

    @Test fun loadMoreAppendsWithoutDuplicateNodeIds() = runTest(dispatcher) {
        val repository = FakeInboxRepository()
        val model = InboxViewModel(repository, MemoryInboxCache())
        model.refresh()
        advanceUntilIdle()
        model.loadMore()
        advanceUntilIdle()
        assertEquals(listOf("reviews", "next"), model.state.value.reviewRequests.items.map { it.nodeId })
    }

    @Test fun cachedRowsRemainVisibleWhenOffline() = runTest(dispatcher) {
        val page = InboxPage(listOf(item("cached")))
        val model = InboxViewModel(FailingInboxRepository(), MemoryInboxCache(page to InboxPage()))
        model.refresh()
        advanceUntilIdle()
        assertEquals("cached", model.state.value.reviewRequests.items.single().nodeId)
        assertEquals("Offline · showing last refresh", model.state.value.reviewRequests.error)
    }
}

private class FakeInboxRepository : InboxRepository {
    override suspend fun load(tab: InboxTab, after: String?): InboxPage = when {
        after != null -> InboxPage(listOf(item("reviews"), item("next")))
        tab == InboxTab.ReviewRequests -> InboxPage(listOf(item("reviews")), "cursor", true)
        else -> InboxPage(listOf(item("authored")))
    }
}
private class FailingInboxRepository : InboxRepository {
    override suspend fun load(tab: InboxTab, after: String?) = error("offline")
}
private class MemoryInboxCache(var value: Pair<InboxPage, InboxPage>? = null) : InboxCache {
    override fun load() = value
    override fun save(reviewRequests: InboxPage, authored: InboxPage) { value = reviewRequests to authored }
    override fun clear() { value = null }
}
private fun item(id: String) = InboxItem(id, "owner/repo", 12, "Title", "url", "author", "2026-07-27", false, 4, 2, 1)
