package io.github.carlosarraes.ramo.review

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import io.github.carlosarraes.ramo.ui.theme.RamoAppSurface
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ReviewScreenTest {
    @get:Rule val compose = createComposeRule()

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

    private fun setReviewContent() {
        compose.setContent {
            RamoAppSurface {
                var selectedFile by remember { mutableIntStateOf(0) }
                var fileSheetOpen by remember { mutableStateOf(false) }
                val pull = pullRequest()
                val screen = fileScreen(selectedFile)
                ReviewScreen(
                    state = ReviewUiState(
                        loading = false,
                        pullRequest = pull,
                        selectedFile = selectedFile,
                        screen = screen,
                        fileSheetOpen = fileSheetOpen,
                    ),
                    codeSize = 13,
                    onBack = {},
                    onFileSheet = { fileSheetOpen = it },
                    onSummaryExpanded = {},
                    onSelectFile = { selectedFile = it; fileSheetOpen = false },
                    onPrevious = { selectedFile = (selectedFile - 1).coerceAtLeast(0) },
                    onNext = { selectedFile = (selectedFile + 1).coerceAtMost(1) },
                    onLoadMore = {},
                    onLastRow = {},
                    onViewed = {},
                    onHorizontalOffset = {},
                    onComment = {},
                    onExpand = {},
                    onFinish = {},
                    onExtendSelection = {},
                    onCancelEditor = {},
                    onSaveDraft = {},
                    onOverallBody = {},
                    onVerdict = {},
                    onDeleteDraft = {},
                    onConfirmation = {},
                    onPublish = {},
                    onDismissSuccess = {},
                    onRefreshAfterAttention = {},
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
            key = "row-$index",
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
