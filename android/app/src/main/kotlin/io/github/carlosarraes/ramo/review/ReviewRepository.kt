package io.github.carlosarraes.ramo.review

import io.github.carlosarraes.ramo.auth.NativeAuthenticator
import io.github.carlosarraes.ramo.uniffi.MobileCommentSide
import io.github.carlosarraes.ramo.uniffi.MobileFileScreen
import io.github.carlosarraes.ramo.uniffi.MobileFileSummary
import io.github.carlosarraes.ramo.uniffi.MobileLineKind
import io.github.carlosarraes.ramo.uniffi.MobilePullRequestDetail
import io.github.carlosarraes.ramo.uniffi.MobileDraftInput
import io.github.carlosarraes.ramo.uniffi.MobileDraftSide
import io.github.carlosarraes.ramo.uniffi.MobileDraftReview
import io.github.carlosarraes.ramo.uniffi.MobileReviewVerdict
import io.github.carlosarraes.ramo.uniffi.createMobileDraft
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

interface ReviewRepository {
    suspend fun open(repository: String, number: Long): PullRequestUi
    suspend fun file(repository: String, number: Long, index: Int, startRow: Long, limit: Long = 400): FileScreenUi
    suspend fun setViewed(pullRequestId: String, path: String, viewed: Boolean)
    suspend fun expand(repository: String, number: Long, index: Int, gapKey: String): FileScreenUi
    suspend fun createDraft(input: DraftInputUi): DraftCommentUi
    suspend fun publish(review: DraftReviewUi, verdict: ReviewVerdictUi)
}

class NativeReviewRepository(
    private val authenticator: NativeAuthenticator,
    private val dispatcher: CoroutineDispatcher = Dispatchers.IO,
) : ReviewRepository {
    override suspend fun open(repository: String, number: Long) = withContext(dispatcher) {
        session().openPullRequest(repository, number.toULong()).toUi()
    }

    override suspend fun file(repository: String, number: Long, index: Int, startRow: Long, limit: Long) =
        withContext(dispatcher) {
            session().fileScreen(repository, number.toULong(), index.toULong(), startRow.toULong(), limit.toULong()).toUi()
        }

    override suspend fun setViewed(pullRequestId: String, path: String, viewed: Boolean) = withContext(dispatcher) {
        session().setFileViewed(pullRequestId, path, viewed)
    }

    override suspend fun expand(repository: String, number: Long, index: Int, gapKey: String) = withContext(dispatcher) {
        session().expandContext(repository, number.toULong(), index.toULong(), gapKey).toUi()
    }

    override suspend fun createDraft(input: DraftInputUi) = withContext(dispatcher) {
        createMobileDraft(
            MobileDraftInput(
                input.repository, input.number.toULong(), input.revision, input.path,
                input.side.toMobile(), input.side.toMobile(), input.startLine.toUInt(), input.endLine.toUInt(),
                input.hunk.toULong(), input.hunk.toULong(), input.contextBefore, input.selectedText,
                input.contextAfter, input.body,
            ),
        ).let { draft ->
            DraftCommentUi(
                draft.id, draft.repository, draft.number.toLong(), draft.capturedRevision, draft.path,
                if (draft.side == MobileDraftSide.LEFT) CommentSideUi.Left else CommentSideUi.Right,
                draft.startLine.toInt(), draft.endLine.toInt(), draft.contextBefore, draft.selectedText,
                draft.contextAfter, draft.body,
            )
        }
    }

    override suspend fun publish(review: DraftReviewUi, verdict: ReviewVerdictUi) = withContext(dispatcher) {
        val mobile = MobileDraftReview(
            review.repository, review.number.toULong(), review.revision, review.body,
            review.comments.map { comment ->
                io.github.carlosarraes.ramo.uniffi.MobileDraftComment(
                    comment.id, comment.repository, comment.number.toULong(), comment.revision, comment.path,
                    comment.side.toMobile(), comment.startLine.toUInt(), comment.endLine.toUInt(),
                    comment.contextBefore, comment.selectedText, comment.contextAfter, comment.body,
                )
            },
        )
        session().publishReview(
            mobile,
            when (verdict) {
                ReviewVerdictUi.Comment -> MobileReviewVerdict.COMMENT
                ReviewVerdictUi.Approve -> MobileReviewVerdict.APPROVE
                ReviewVerdictUi.RequestChanges -> MobileReviewVerdict.REQUEST_CHANGES
            },
        )
    }

    private fun session() = checkNotNull(authenticator.session()) { "GitHub session expired" }
}

private fun CommentSideUi.toMobile() = if (this == CommentSideUi.Left) MobileDraftSide.LEFT else MobileDraftSide.RIGHT

private fun MobilePullRequestDetail.toUi() = PullRequestUi(
    nodeId, repository, number.toLong(), title, authorLogin, viewerLogin, baseRef, headRef,
    capturedRevision, additions.toLong(), deletions.toLong(), files.map { it.toUi() },
)

private fun MobileFileSummary.toUi() = FileSummaryUi(
    path, previousPath, status, additions.toLong(), deletions.toLong(), viewed, binary,
)

private fun MobileFileScreen.toUi() = FileScreenUi(
    repository,
    number.toLong(),
    title,
    pullRequestId,
    additions.toLong(),
    deletions.toLong(),
    fileIndex.toInt(),
    fileCount.toInt(),
    viewedCount.toInt(),
    file.toUi(),
    rows.map { row ->
        DiffRowUi(
            row.key,
            row.hunkIndex.toLong(),
            row.oldLine?.toInt(),
            row.newLine?.toInt(),
            when (row.kind) {
                MobileLineKind.CONTEXT -> LineKindUi.Context
                MobileLineKind.ADDITION -> LineKindUi.Addition
                MobileLineKind.DELETION -> LineKindUi.Deletion
                MobileLineKind.HUNK -> LineKindUi.Hunk
            },
            row.spans.map { span ->
                SyntaxSpanUi(
                    span.text,
                    (0xff000000L or (span.red.toLong() shl 16) or (span.green.toLong() shl 8) or span.blue.toLong()),
                    span.bold,
                    span.italic,
                    span.underline,
                )
            },
            row.commentable,
        )
    },
    nextRow?.toLong(),
    threads.map { thread ->
        ReviewThreadUi(
            thread.id, thread.path,
            when (thread.side) {
                MobileCommentSide.LEFT -> CommentSideUi.Left
                MobileCommentSide.RIGHT -> CommentSideUi.Right
                null -> null
            },
            thread.startLine?.toInt(), thread.endLine?.toInt(), thread.resolved, thread.outdated,
            thread.url,
            thread.comments.map { ThreadCommentUi(it.author, it.body, it.createdAt, it.url) },
        )
    },
)
