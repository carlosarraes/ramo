package io.github.carlosarraes.ramo.review

import android.content.Context
import io.github.carlosarraes.ramo.security.EncryptedBlobStore
import io.github.carlosarraes.ramo.uniffi.MobileDraftComment
import io.github.carlosarraes.ramo.uniffi.MobileDraftReview
import io.github.carlosarraes.ramo.uniffi.MobileDraftSide
import io.github.carlosarraes.ramo.uniffi.decodeDraftReview
import io.github.carlosarraes.ramo.uniffi.encodeDraftReview

interface ReviewDraftStore {
    fun load(repository: String, number: Long): DraftReviewUi?
    fun save(review: DraftReviewUi)
    fun clear(repository: String, number: Long)
    fun clearAll()
}

class NoopDraftStore : ReviewDraftStore {
    override fun load(repository: String, number: Long) = null
    override fun save(review: DraftReviewUi) = Unit
    override fun clear(repository: String, number: Long) = Unit
    override fun clearAll() = Unit
}

class SecureDraftStore(private val context: Context) : ReviewDraftStore {
    override fun load(repository: String, number: Long): DraftReviewUi? = runCatching {
        val bytes = blob(repository, number).read() ?: return null
        decodeDraftReview(bytes).toUi()
    }.getOrNull()

    override fun save(review: DraftReviewUi) {
        blob(review.repository, review.number).write(encodeDraftReview(review.toMobile()))
    }

    override fun clear(repository: String, number: Long) = blob(repository, number).clear()

    override fun clearAll() {
        context.filesDir.listFiles { file -> file.name.startsWith("review-") && file.name.endsWith(".bin") }
            .orEmpty().forEach { it.delete() }
    }

    private fun blob(repository: String, number: Long) = EncryptedBlobStore(
        context,
        "ramo.mobile.drafts.v1",
        "review-${repository.hashCode().toUInt().toString(16)}-$number.bin",
    )
}

private fun DraftReviewUi.toMobile() = MobileDraftReview(repository, number.toULong(), revision, body, comments.map { it.toMobile() })
private fun DraftCommentUi.toMobile() = MobileDraftComment(
    id, repository, number.toULong(), revision, path,
    if (side == CommentSideUi.Left) MobileDraftSide.LEFT else MobileDraftSide.RIGHT,
    startLine.toUInt(), endLine.toUInt(), contextBefore, selectedText, contextAfter, body,
)
private fun MobileDraftReview.toUi() = DraftReviewUi(repository, number.toLong(), capturedRevision, body, comments.map { it.toUi() })
private fun MobileDraftComment.toUi() = DraftCommentUi(
    id, repository, number.toLong(), capturedRevision, path,
    if (side == MobileDraftSide.LEFT) CommentSideUi.Left else CommentSideUi.Right,
    startLine.toInt(), endLine.toInt(), contextBefore, selectedText, contextAfter, body,
)
