package io.github.carlosarraes.ramo.review

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
class ReviewViewModelTest {
    private val dispatcher = StandardTestDispatcher()
    @BeforeTest fun setup() = Dispatchers.setMain(dispatcher)
    @AfterTest fun teardown() = Dispatchers.resetMain()

    @Test fun opensFirstFileAndNavigationIsExplicitAndBounded() = runTest(dispatcher) {
        val model = ReviewViewModel(FakeReviewRepository(), "ramo/ramo", 7)
        advanceUntilIdle()
        assertEquals(0, model.state.value.selectedFile)
        model.previousFile()
        assertEquals(0, model.state.value.selectedFile)
        model.nextFile()
        advanceUntilIdle()
        assertEquals(1, model.state.value.selectedFile)
        model.nextFile()
        assertEquals(1, model.state.value.selectedFile)
    }

    @Test fun rowPagesDeduplicateAndAutoViewedOnlyAtTheRealEnd() = runTest(dispatcher) {
        val repository = FakeReviewRepository()
        val model = ReviewViewModel(repository, "ramo/ramo", 7)
        advanceUntilIdle()
        model.lastRowVisible()
        advanceUntilIdle()
        assertTrue(repository.viewedCalls.isEmpty())
        model.loadMoreRows()
        advanceUntilIdle()
        assertEquals(listOf("a", "b"), model.state.value.screen!!.rows.map { it.key })
        model.lastRowVisible()
        advanceUntilIdle()
        assertEquals(listOf(true), repository.viewedCalls)
    }

    @Test fun failedViewedMutationRollsBack() = runTest(dispatcher) {
        val repository = FakeReviewRepository(failViewed = true)
        val model = ReviewViewModel(repository, "ramo/ramo", 7)
        advanceUntilIdle()
        model.setViewed(true)
        assertTrue(model.state.value.screen!!.file.viewed)
        advanceUntilIdle()
        assertFalse(model.state.value.screen!!.file.viewed)
        assertEquals("Could not sync viewed state", model.state.value.error)
    }

    @Test fun remembersHorizontalOffsetPerFile() = runTest(dispatcher) {
        val model = ReviewViewModel(FakeReviewRepository(), "ramo/ramo", 7)
        advanceUntilIdle()
        model.setHorizontalOffset(88)
        model.nextFile()
        advanceUntilIdle()
        model.setHorizontalOffset(12)
        assertEquals(mapOf(0 to 88, 1 to 12), model.state.value.horizontalOffsets)
    }

    @Test fun selectingAFileClosesTheFileSheet() = runTest(dispatcher) {
        val model = ReviewViewModel(FakeReviewRepository(), "ramo/ramo", 7)
        advanceUntilIdle()
        model.setFileSheet(true)
        model.selectFile(1)
        advanceUntilIdle()
        assertFalse(model.state.value.fileSheetOpen)
        assertEquals(1, model.state.value.selectedFile)
    }

    @Test fun summaryExpansionIsExplicit() = runTest(dispatcher) {
        val model = ReviewViewModel(FakeReviewRepository(), "ramo/ramo", 7)
        advanceUntilIdle()
        model.setSummaryExpanded(true)
        assertTrue(model.state.value.summaryExpanded)
    }

    @Test fun savesMultilineDraftOnlyOnExplicitSave() = runTest(dispatcher) {
        val store = MemoryReviewDraftStore()
        val model = ReviewViewModel(FakeReviewRepository(), "ramo/ramo", 7, store)
        advanceUntilIdle()
        model.beginComment(model.state.value.screen!!.rows.first())
        assertTrue(model.state.value.drafts.isEmpty())
        model.saveDraft("first\nsecond")
        advanceUntilIdle()
        assertEquals("first\nsecond", model.state.value.drafts.single().body)
        assertEquals("first\nsecond", store.value!!.comments.single().body)
    }

    @Test fun successfulPublishClearsOnlyThisReview() = runTest(dispatcher) {
        val repository = FakeReviewRepository()
        val store = MemoryReviewDraftStore()
        val model = ReviewViewModel(repository, "ramo/ramo", 7, store)
        advanceUntilIdle()
        model.setOverallBody("Looks good")
        model.setVerdict(ReviewVerdictUi.Approve)
        model.publish()
        advanceUntilIdle()
        assertEquals(1, repository.publishCalls)
        assertEquals(null, store.value)
        assertEquals("Review published", model.state.value.success)
    }

    @Test fun unknownOpenFailureDoesNotLeakItsRuntimeMessage() = runTest(dispatcher) {
        val model = ReviewViewModel(
            FakeReviewRepository(openFailure = IllegalStateException("event loop thread panicked")),
            "ramo/ramo",
            7,
        )

        advanceUntilIdle()

        assertEquals("Could not load this pull request", model.state.value.error)
        assertFalse(model.state.value.error!!.contains("panicked"))
    }
}

private class FakeReviewRepository(
    private val failViewed: Boolean = false,
    private val openFailure: Throwable? = null,
) : ReviewRepository {
    val viewedCalls = mutableListOf<Boolean>()
    var publishCalls = 0
    override suspend fun open(repository: String, number: Long): PullRequestUi {
        openFailure?.let { throw it }
        return pull()
    }
    override suspend fun file(repository: String, number: Long, index: Int, startRow: Long, limit: Long): FileScreenUi {
        val rows = if (startRow == 0L) listOf(row("a")) else listOf(row("a"), row("b"))
        return screen(index, rows, if (startRow == 0L) 1 else null)
    }
    override suspend fun setViewed(pullRequestId: String, path: String, viewed: Boolean) {
        viewedCalls += viewed
        if (failViewed) error("no")
    }
    override suspend fun expand(repository: String, number: Long, index: Int, gapKey: String) =
        screen(index, listOf(row("expanded")), null)
    override suspend fun createDraft(input: DraftInputUi) = DraftCommentUi(
        "id", input.repository, input.number, input.revision, input.path, input.side,
        input.startLine, input.endLine, input.contextBefore, input.selectedText, input.contextAfter, input.body,
    )
    override suspend fun publish(review: DraftReviewUi, verdict: ReviewVerdictUi) { publishCalls += 1 }
}

private class MemoryReviewDraftStore(var value: DraftReviewUi? = null) : ReviewDraftStore {
    override fun load(repository: String, number: Long) = value
    override fun save(review: DraftReviewUi) { value = review }
    override fun clear(repository: String, number: Long) { value = null }
    override fun clearAll() { value = null }
}

private fun pull() = PullRequestUi("node", "ramo/ramo", 7, "Title", "a", "v", "main", "head", "sha", 2, 1, listOf(file("a.rs"), file("b.rs")))
private fun file(path: String) = FileSummaryUi(path, null, "modified", 1, 1, false, false)
private fun row(key: String) = DiffRowUi(key, 0, 1, 1, LineKindUi.Context, listOf(SyntaxSpanUi(key, 0xffc0caf5, false, false, false)), true)
private fun screen(index: Int, rows: List<DiffRowUi>, next: Long?) = FileScreenUi("ramo/ramo", 7, "Title", "node", 2, 1, index, 2, 0, file(if (index == 0) "a.rs" else "b.rs"), rows, next, emptyList())
