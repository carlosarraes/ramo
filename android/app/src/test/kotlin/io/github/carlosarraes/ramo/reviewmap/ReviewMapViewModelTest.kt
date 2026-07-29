package io.github.carlosarraes.ramo.reviewmap

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
import kotlin.test.assertTrue

@OptIn(ExperimentalCoroutinesApi::class)
class ReviewMapViewModelTest {
    private val dispatcher = StandardTestDispatcher()
    @BeforeTest fun setup() = Dispatchers.setMain(dispatcher)
    @AfterTest fun teardown() = Dispatchers.resetMain()

    @Test
    fun exactMapAppearsBeforeEnrichmentAndKeepsExpansion() = runTest(dispatcher) {
        val repository = FakeRepository()
        val model = ReviewMapViewModel(repository, "owner/repo", 7)
        advanceUntilIdle()
        assertEquals(ReviewMapPhase.Enriched, model.state.value.phase)
        model.toggleGroup("tests")
        assertTrue("tests" in model.state.value.expandedGroups)
        assertEquals("AI summary", model.state.value.map!!.groups.first().summary)
    }

    private class FakeRepository : ReviewMapRepository {
        private val exact = map(null)
        override suspend fun exact(repository: String, number: Long) = exact
        override fun isPaired() = true
        override suspend fun resolve(request: ReviewMapResolveRequest) =
            ReviewMapServerResult("job", ReviewMapPhase.Enriched, map("AI summary"))
        override suspend fun poll(jobId: String) = error("not needed")
        override suspend fun retry(jobId: String) = error("not needed")
    }

    companion object {
        private fun map(summary: String?): ReviewMapUi {
            val file = ReviewMapFileUi("file", "tests/test_api.py", 3, 1, ReviewFileKindUi.Test)
            val group = ReviewMapGroupUi("tests", "Tests", ReviewFileKindUi.Test, listOf("file"), 3, 1, true, summary)
            return ReviewMapUi("owner/repo", 7, "base", "head", 3, 1, listOf(group), listOf(file))
        }
    }
}
