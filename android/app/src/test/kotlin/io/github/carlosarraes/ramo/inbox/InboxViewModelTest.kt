package io.github.carlosarraes.ramo.inbox

import io.github.carlosarraes.ramo.errors.FailureKind
import io.github.carlosarraes.ramo.uniffi.MobileException
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
import kotlin.test.assertFalse
import kotlin.test.assertTrue

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
        val model = InboxViewModel(
            FailingInboxRepository(),
            MemoryInboxCache(InboxCacheValue(page, InboxPage(), 99L)),
        )
        model.refresh()
        advanceUntilIdle()
        assertEquals("cached", model.state.value.reviewRequests.items.single().nodeId)
        assertEquals(
            "Offline · showing last refresh",
            model.state.value.reviewRequests.failure?.message,
        )
    }

    @Test fun unavailableOrganizationAccessUsesApprovalGuidance() = runTest(dispatcher) {
        val model = InboxViewModel(AccessUnavailableInboxRepository(), MemoryInboxCache())

        model.refresh()
        advanceUntilIdle()

        val failure = model.state.value.reviewRequests.failure!!
        assertEquals(FailureKind.AccessUnavailable, failure.kind)
        assertFalse(failure.message.contains("Forbidden"))
    }

    @Test fun queryFiltersRepositoryTitleAndAuthorCaseInsensitively() = runTest(dispatcher) {
        val model = InboxViewModel(FakeInboxRepository(), MemoryInboxCache(), nowMillis = { 42L })
        model.refresh()
        advanceUntilIdle()
        model.setQuery("OWNER")
        assertEquals(listOf("reviews"), model.visibleItems().map(InboxItem::nodeId))
        model.setQuery("missing")
        assertTrue(model.visibleItems().isEmpty())
    }

    @Test fun successfulRefreshRecordsAndCachesItsTimestamp() = runTest(dispatcher) {
        val cache = MemoryInboxCache()
        val model = InboxViewModel(FakeInboxRepository(), cache, nowMillis = { 1234L })
        model.refresh()
        advanceUntilIdle()
        assertEquals(1234L, model.state.value.reviewRequests.refreshedAtEpochMillis)
        assertEquals(1234L, cache.value?.refreshedAtEpochMillis)
    }

    @Test fun cachedFailureCanBeDismissedWithoutRemovingRows() = runTest(dispatcher) {
        val cached = InboxCacheValue(InboxPage(listOf(item("cached"))), InboxPage(), 99L)
        val model = InboxViewModel(FailingInboxRepository(), MemoryInboxCache(cached))
        model.refresh()
        advanceUntilIdle()
        model.dismissFailure(InboxTab.ReviewRequests)
        assertEquals(null, model.state.value.reviewRequests.failure)
        assertEquals("cached", model.state.value.reviewRequests.items.single().nodeId)
        assertTrue(model.state.value.reviewRequests.fromCache)
        assertEquals(99L, model.state.value.reviewRequests.refreshedAtEpochMillis)
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
private class AccessUnavailableInboxRepository : InboxRepository {
    override suspend fun load(tab: InboxTab, after: String?): InboxPage {
        throw MobileException.AccessUnavailable()
    }
}
private class MemoryInboxCache(var value: InboxCacheValue? = null) : InboxCache {
    override fun load() = value
    override fun save(value: InboxCacheValue) { this.value = value }
    override fun clear() { value = null }
}
private fun item(id: String) = InboxItem(id, "owner/repo", 12, "Title", "url", "author", "2026-07-27", false, 4, 2, 1)
