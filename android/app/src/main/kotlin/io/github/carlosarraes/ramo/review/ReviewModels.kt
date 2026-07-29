package io.github.carlosarraes.ramo.review

data class PullRequestUi(
    val nodeId: String,
    val repository: String,
    val number: Long,
    val title: String,
    val author: String,
    val viewer: String,
    val baseRef: String,
    val headRef: String,
    val revision: String,
    val additions: Long,
    val deletions: Long,
    val files: List<FileSummaryUi>,
)

data class FileSummaryUi(
    val path: String,
    val previousPath: String?,
    val status: String,
    val additions: Long,
    val deletions: Long,
    val viewed: Boolean,
    val binary: Boolean,
)

enum class LineKindUi { Context, Addition, Deletion, Hunk }
enum class CommentSideUi { Left, Right }

data class SyntaxSpanUi(
    val text: String,
    val color: Long,
    val bold: Boolean,
    val italic: Boolean,
    val underline: Boolean,
)

data class DiffRowUi(
    val key: String,
    val hunkIndex: Long,
    val oldLine: Int?,
    val newLine: Int?,
    val kind: LineKindUi,
    val spans: List<SyntaxSpanUi>,
    val commentable: Boolean,
)

data class ThreadCommentUi(val author: String, val body: String, val createdAt: String, val url: String)
data class ReviewThreadUi(
    val id: String,
    val path: String,
    val side: CommentSideUi?,
    val startLine: Int?,
    val endLine: Int?,
    val resolved: Boolean,
    val outdated: Boolean,
    val url: String,
    val comments: List<ThreadCommentUi>,
)

data class FileScreenUi(
    val repository: String,
    val number: Long,
    val title: String,
    val pullRequestId: String,
    val additions: Long,
    val deletions: Long,
    val fileIndex: Int,
    val fileCount: Int,
    val viewedCount: Int,
    val file: FileSummaryUi,
    val rows: List<DiffRowUi>,
    val nextRow: Long?,
    val threads: List<ReviewThreadUi>,
)

data class ReviewUiState(
    val loading: Boolean = true,
    val pullRequest: PullRequestUi? = null,
    val selectedFile: Int = 0,
    val screen: FileScreenUi? = null,
    val fileSheetOpen: Boolean = false,
    val summaryExpanded: Boolean = false,
    val error: String? = null,
    val horizontalOffsets: Map<Int, Int> = emptyMap(),
    val finishing: Boolean = false,
    val drafts: List<DraftCommentUi> = emptyList(),
    val editor: DraftEditorUi? = null,
    val overallBody: String = "",
    val verdict: ReviewVerdictUi = ReviewVerdictUi.Comment,
    val confirmation: Boolean = false,
    val publishing: Boolean = false,
    val success: String? = null,
    val needsAttention: Boolean = false,
)

enum class ReviewVerdictUi { Comment, Approve, RequestChanges }

data class DraftInputUi(
    val repository: String,
    val number: Long,
    val revision: String,
    val path: String,
    val side: CommentSideUi,
    val startLine: Int,
    val endLine: Int,
    val hunk: Long,
    val contextBefore: List<String>,
    val selectedText: List<String>,
    val contextAfter: List<String>,
    val body: String,
)

data class DraftCommentUi(
    val id: String,
    val repository: String,
    val number: Long,
    val revision: String,
    val path: String,
    val side: CommentSideUi,
    val startLine: Int,
    val endLine: Int,
    val contextBefore: List<String>,
    val selectedText: List<String>,
    val contextAfter: List<String>,
    val body: String,
) {
    val label: String get() = "${if (side == CommentSideUi.Left) "L" else "R"}${if (startLine == endLine) endLine else "$startLine–$endLine"}"
}

data class DraftReviewUi(
    val repository: String,
    val number: Long,
    val revision: String,
    val body: String,
    val comments: List<DraftCommentUi>,
)

data class DraftEditorUi(
    val rowKey: String,
    val side: CommentSideUi,
    val hunk: Long,
    val startLine: Int,
    val endLine: Int,
) {
    val label: String get() = "${if (side == CommentSideUi.Left) "L" else "R"}${if (startLine == endLine) endLine else "$startLine–$endLine"}"
}
