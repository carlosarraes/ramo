package io.github.carlosarraes.ramo.review

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertHasClickAction
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.performClick
import io.github.carlosarraes.ramo.ui.theme.RamoAppSurface
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import kotlin.test.assertEquals

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ReviewScreenTest {
    @get:Rule val compose = createComposeRule()

    @Test
    fun chromeItemsDoNotCountAsVisibleDiffRows() {
        assertEquals(
            0,
            lastVisibleDiffRowIndex(
                visibleKeys = listOf(0, 1, "a"),
                rowIndices = mapOf("a" to 0, "b" to 1),
            ),
        )
    }

    @Test
    fun reviewIsOneFileAtATimeAndFileProgressOpensTheSheet() {
        setReviewContent()

        compose.onNodeWithText("a.rs").assertIsDisplayed()
        compose.onNodeWithText("1 / 2").performClick()
        compose.onNodeWithText("b.rs").assertIsDisplayed()
        compose.onNodeWithText("Previous file").assertIsNotEnabled()
        compose.onNodeWithText("Next file").assertIsEnabled()
    }

    @Test
    fun codeAndBottomNavigationRemainInsideSafeDrawingBounds() {
        setReviewContent()

        compose.onNodeWithTag("review-top-title").assertIsDisplayed()
        compose.onNodeWithTag("review-bottom-nav").assertIsDisplayed()
        compose.onNodeWithTag("diff-list").assertIsDisplayed()
    }

    @Test
    fun selectingCodeDoesNotOpenKeyboardUntilCommentIsPressed() {
        setReviewContent()

        compose.onNodeWithTag("diff-row-a").performClick()
        compose.onNodeWithTag("comment-selection").assertIsDisplayed()
        compose.onNodeWithText("Draft comment").assertDoesNotExist()
        compose.onNodeWithText("Comment").performClick()
        compose.onNodeWithText("Draft comment").assertIsDisplayed()
    }

    @Test
    fun savedDraftIsInlineAndCanBeReopenedForEditing() {
        val draft = draftComment("Needs explanation")
        setReviewContent(drafts = listOf(draft))

        compose.onNodeWithText("Draft · R1").assertIsDisplayed()
        compose.onNodeWithText("Needs explanation").assertIsDisplayed()
        compose.onNodeWithText("Edit").performClick()
        compose.onNodeWithText("Draft comment").assertIsDisplayed()
        compose.onAllNodesWithText("Needs explanation").assertCountEquals(2)
    }

    @Test
    fun viewedNoticeOffersUndo() {
        setReviewContent(notice = ReviewNoticeUi(1, "Marked viewed", 0))

        compose.onNodeWithText("Marked viewed").assertIsDisplayed()
        compose.onNodeWithText("Undo").assertHasClickAction()
    }

    @Test
    fun unavailablePullStaysInsideReviewWithRetryAndBack() {
        setReviewContent(
            loading = false,
            pull = null,
            screen = null,
            error = "Could not load this pull request",
        )

        compose.onNodeWithText("Could not load this pull request").assertIsDisplayed()
        compose.onNodeWithText("Retry").assertHasClickAction()
        compose.onNodeWithText("Back").assertHasClickAction()
    }

    private fun setReviewContent(
        notice: ReviewNoticeUi? = null,
        loading: Boolean = false,
        pull: PullRequestUi? = pullRequest(),
        screen: FileScreenUi? = fileScreen(0),
        error: String? = null,
        drafts: List<DraftCommentUi> = emptyList(),
    ) {
        compose.setContent {
            RamoAppSurface {
                var selectedFile by remember { mutableIntStateOf(0) }
                var fileSheetOpen by remember { mutableStateOf(false) }
                var selection by remember { mutableStateOf<LineSelectionUi?>(null) }
                var editor by remember { mutableStateOf<DraftEditorUi?>(null) }
                ReviewScreen(
                    state = ReviewUiState(
                        loading = loading,
                        pullRequest = pull,
                        selectedFile = selectedFile,
                        screen = screen,
                        fileSheetOpen = fileSheetOpen,
                        selection = selection,
                        editor = editor,
                        drafts = drafts,
                        notice = notice,
                        error = error,
                    ),
                    codeSize = 13,
                    onBack = {},
                    onRetry = {},
                    onDismissError = {},
                    onFileSheet = { fileSheetOpen = it },
                    onSummaryExpanded = {},
                    onSelectFile = { selectedFile = it; fileSheetOpen = false },
                    onPrevious = { selectedFile = (selectedFile - 1).coerceAtLeast(0) },
                    onNext = { selectedFile = (selectedFile + 1).coerceAtMost(1) },
                    onLoadMore = {},
                    onLastRow = {},
                    onViewed = {},
                    onHorizontalOffset = {},
                    onSelectLine = { row ->
                        val side = if (row.kind == LineKindUi.Deletion) CommentSideUi.Left else CommentSideUi.Right
                        val line = if (side == CommentSideUi.Left) row.oldLine else row.newLine
                        selection = line?.let { LineSelectionUi(side, row.hunkIndex, it, it) }
                    },
                    onOpenComment = { selection?.let { editor = DraftEditorUi(it) } },
                    onEditDraft = { draft ->
                        val selection = LineSelectionUi(draft.side, 0, draft.startLine, draft.endLine)
                        editor = DraftEditorUi(selection, draft.id, draft.body)
                    },
                    onClearSelection = { selection = null },
                    onExpand = {},
                    onFinish = {},
                    onCancelEditor = { editor = null },
                    onSaveDraft = {},
                    onOverallBody = {},
                    onVerdict = {},
                    onDeleteDraft = {},
                    onConfirmation = {},
                    onPublish = {},
                    onDismissSuccess = {},
                    onRefreshAfterAttention = {},
                    onUndoViewed = {},
                    onDismissNotice = {},
                )
            }
        }
    }
}

private fun pullRequest() = PullRequestUi(
    nodeId = "node",
    repository = "ramo/ramo",
    number = 7,
    title = "Readable mobile reviews",
    author = "author",
    viewer = "reviewer",
    baseRef = "main",
    headRef = "feature",
    revision = "sha",
    additions = 42,
    deletions = 8,
    files = listOf(fileSummary("a.rs"), fileSummary("b.rs")),
)

private fun fileScreen(index: Int) = FileScreenUi(
    repository = "ramo/ramo",
    number = 7,
    title = "Readable mobile reviews",
    pullRequestId = "node",
    additions = 42,
    deletions = 8,
    fileIndex = index,
    fileCount = 2,
    viewedCount = 0,
    file = fileSummary(if (index == 0) "a.rs" else "b.rs"),
    rows = listOf(
        DiffRowUi(
            key = "a",
            hunkIndex = 0,
            oldLine = 1,
            newLine = 1,
            kind = LineKindUi.Context,
            spans = listOf(SyntaxSpanUi("fn main() {}", 0xffc0caf5, false, false, false)),
            commentable = true,
        ),
    ),
    nextRow = null,
    threads = emptyList(),
)

private fun fileSummary(path: String) = FileSummaryUi(
    path = path,
    previousPath = null,
    status = "modified",
    additions = 21,
    deletions = 4,
    viewed = false,
    binary = false,
)

private fun draftComment(body: String) = DraftCommentUi(
    id = "draft-1",
    repository = "ramo/ramo",
    number = 7,
    revision = "sha",
    path = "a.rs",
    side = CommentSideUi.Right,
    startLine = 1,
    endLine = 1,
    contextBefore = emptyList(),
    selectedText = listOf("fn main() {}"),
    contextAfter = emptyList(),
    body = body,
)
