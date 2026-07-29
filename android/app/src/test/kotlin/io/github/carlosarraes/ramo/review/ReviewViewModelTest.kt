package io.github.carlosarraes.ramo.review

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
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

    @Test fun autoViewedAtEndOffersUndoAndUndoWins() = runTest(dispatcher) {
        val repository = FakeReviewRepository(firstPageComplete = true)
        val model = ReviewViewModel(repository, "ramo/ramo", 7)
        advanceUntilIdle()
        model.lastRowVisible()
        assertEquals(0, model.state.value.notice!!.undoViewedFile)
        model.undoViewed()
        advanceUntilIdle()
        assertFalse(model.state.value.pullRequest!!.files[0].viewed)
        assertEquals(listOf(false), repository.viewedCalls.takeLast(1))
    }

    @Test fun undoWaitsForTheBlockingViewedWriteSoRemoteStateEndsFalse() = runTest(dispatcher) {
        val repository = FakeReviewRepository(
            firstPageComplete = true,
            viewedDelays = mapOf(true to 200L, false to 10L),
        )
        val model = ReviewViewModel(repository, "ramo/ramo", 7)
        advanceUntilIdle()

        model.lastRowVisible()
        model.undoViewed()
        advanceUntilIdle()

        assertEquals(listOf(true, false), repository.viewedCalls)
        assertFalse(repository.remoteViewed)
    }

    @Test fun twoFailedViewedWritesRestoreTheLastConfirmedRemoteState() = runTest(dispatcher) {
        val repository = FakeReviewRepository(
            failViewed = true,
            firstPageComplete = true,
            viewedDelays = mapOf(true to 200L, false to 10L),
        )
        val model = ReviewViewModel(repository, "ramo/ramo", 7)
        advanceUntilIdle()

        model.lastRowVisible()
        model.undoViewed()
        advanceUntilIdle()

        assertEquals(listOf(true, false), repository.viewedCalls)
        assertFalse(repository.remoteViewed)
        assertFalse(model.state.value.pullRequest!!.files[0].viewed)
    }

    @Test fun retryWaitsForANonCancellableViewedWriteBeforeReloading() = runTest(dispatcher) {
        val repository = FakeReviewRepository(
            firstPageComplete = true,
            viewedDelays = mapOf(true to 200L),
            openUsesRemoteViewed = true,
        )
        val model = ReviewViewModel(repository, "ramo/ramo", 7)
        advanceUntilIdle()

        model.setViewed(true)
        model.open()
        advanceUntilIdle()

        assertEquals(listOf(true), repository.viewedCalls)
        assertTrue(model.state.value.pullRequest!!.files[0].viewed)
    }

    @Test fun navigatingBeforeTheRealEndNeverOffersUndo() = runTest(dispatcher) {
        val model = ReviewViewModel(FakeReviewRepository(), "ramo/ramo", 7)
        advanceUntilIdle()
        model.nextFile()
        advanceUntilIdle()
        assertEquals(null, model.state.value.notice)
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

    @Test fun selectingTheCurrentFileAlsoClosesTheFileSheet() = runTest(dispatcher) {
        val model = ReviewViewModel(FakeReviewRepository(), "ramo/ramo", 7)
        advanceUntilIdle()
        model.setFileSheet(true)
        model.selectFile(0)
        assertFalse(model.state.value.fileSheetOpen)
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
        model.selectLine(model.state.value.screen!!.rows.first())
        model.openComment()
        assertTrue(model.state.value.drafts.isEmpty())
        model.saveDraft("first\nsecond")
        advanceUntilIdle()
        assertEquals("first\nsecond", model.state.value.drafts.single().body)
        assertEquals("first\nsecond", store.value!!.comments.single().body)
    }

    @Test fun tapsSelectThenExtendAContiguousCompatibleRange() = runTest(dispatcher) {
        val model = ReviewViewModel(FakeReviewRepository(rows = selectableRows()), "ramo/ramo", 7)
        advanceUntilIdle()
        val rows = model.state.value.screen!!.rows
        model.selectLine(rows[0])
        model.selectLine(rows[2])
        assertEquals(LineSelectionUi(CommentSideUi.Right, 0, 1, 3), model.state.value.selection)
        assertEquals(null, model.state.value.editor)
    }

    @Test fun incompatibleTapStartsANewSingleLineSelection() = runTest(dispatcher) {
        val model = ReviewViewModel(FakeReviewRepository(rows = mixedSideRows()), "ramo/ramo", 7)
        advanceUntilIdle()
        val rows = model.state.value.screen!!.rows
        model.selectLine(rows[0])
        model.selectLine(rows[1])
        assertEquals(CommentSideUi.Left, model.state.value.selection!!.side)
        assertEquals(model.state.value.selection!!.startLine, model.state.value.selection!!.endLine)
    }

    @Test fun saveClearsSelectionButCancelKeepsIt() = runTest(dispatcher) {
        val model = ReviewViewModel(FakeReviewRepository(), "ramo/ramo", 7)
        advanceUntilIdle()
        model.selectLine(model.state.value.screen!!.rows.first())
        model.openComment()
        model.cancelEditor()
        assertTrue(model.state.value.selection != null)
        model.openComment()
        model.saveDraft("Please simplify")
        advanceUntilIdle()
        assertEquals(null, model.state.value.selection)
    }

    @Test fun savedDraftCanBeReopenedAndReplacedInline() = runTest(dispatcher) {
        val model = ReviewViewModel(FakeReviewRepository(), "ramo/ramo", 7)
        advanceUntilIdle()
        model.selectLine(model.state.value.screen!!.rows.first())
        model.openComment()
        model.saveDraft("Needs explanation")
        advanceUntilIdle()

        val draft = model.state.value.drafts.single()
        model.editDraft(draft)
        assertEquals("Needs explanation", model.state.value.editor!!.initialBody)
        model.saveDraft("Clear now")
        advanceUntilIdle()

        assertEquals(listOf("Clear now"), model.state.value.drafts.map(DraftCommentUi::body))
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

    @Test fun failedOpenCanBeDismissedAndRetriedWithoutASecondModel() = runTest(dispatcher) {
        val repository = FlakyOpenReviewRepository()
        val model = ReviewViewModel(repository, "ramo/ramo", 7)
        advanceUntilIdle()
        assertEquals("Could not load this pull request", model.state.value.error)
        model.dismissError()
        assertEquals(null, model.state.value.error)
        model.open()
        advanceUntilIdle()
        assertEquals("Title", model.state.value.pullRequest!!.title)
    }
}

private class FlakyOpenReviewRepository : ReviewRepository by FakeReviewRepository() {
    private var attempts = 0

    override suspend fun open(repository: String, number: Long): PullRequestUi {
        attempts += 1
        if (attempts == 1) error("temporary failure")
        return pull()
    }
}

private class FakeReviewRepository(
    private val failViewed: Boolean = false,
    private val openFailure: Throwable? = null,
    private val rows: List<DiffRowUi>? = null,
    private val firstPageComplete: Boolean = false,
    private val viewedDelays: Map<Boolean, Long> = emptyMap(),
    private val openUsesRemoteViewed: Boolean = false,
) : ReviewRepository {
    val viewedCalls = mutableListOf<Boolean>()
    var remoteViewed = false
    var publishCalls = 0
    override suspend fun open(repository: String, number: Long): PullRequestUi {
        openFailure?.let { throw it }
        val pull = pull()
        return if (openUsesRemoteViewed) {
            pull.copy(files = pull.files.map { it.copy(viewed = remoteViewed) })
        } else {
            pull
        }
    }
    override suspend fun file(repository: String, number: Long, index: Int, startRow: Long, limit: Long): FileScreenUi {
        val pageRows = rows ?: if (startRow == 0L) listOf(row("a")) else listOf(row("a"), row("b"))
        val nextRow = when {
            rows != null || firstPageComplete || startRow != 0L -> null
            else -> 1L
        }
        return screen(index, pageRows, nextRow)
    }
    override suspend fun setViewed(pullRequestId: String, path: String, viewed: Boolean) {
        viewedDelays[viewed]?.let { delayMillis ->
            withContext(NonCancellable) { delay(delayMillis) }
        }
        viewedCalls += viewed
        remoteViewed = viewed
        if (failViewed) error("no")
    }
    override suspend fun expand(repository: String, number: Long, index: Int, gapKey: String) =
        screen(index, listOf(row("expanded")), null)
    override suspend fun createDraft(input: DraftInputUi) = DraftCommentUi(
        "id-${input.body}", input.repository, input.number, input.revision, input.path, input.side,
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
private fun selectableRows() = (1..3).map { line ->
    DiffRowUi(
        key = "right-$line",
        hunkIndex = 0,
        oldLine = null,
        newLine = line,
        kind = LineKindUi.Addition,
        spans = listOf(SyntaxSpanUi("line $line", 0xffc0caf5, false, false, false)),
        commentable = true,
    )
}
private fun mixedSideRows() = listOf(
    selectableRows().first(),
    DiffRowUi(
        key = "left-9",
        hunkIndex = 0,
        oldLine = 9,
        newLine = null,
        kind = LineKindUi.Deletion,
        spans = listOf(SyntaxSpanUi("removed", 0xffc0caf5, false, false, false)),
        commentable = true,
    ),
)
private fun screen(index: Int, rows: List<DiffRowUi>, next: Long?) = FileScreenUi("ramo/ramo", 7, "Title", "node", 2, 1, index, 2, 0, file(if (index == 0) "a.rs" else "b.rs"), rows, next, emptyList())
