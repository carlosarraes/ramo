package io.github.carlosarraes.ramo.reviewmap

import io.github.carlosarraes.ramo.auth.NativeAuthenticator
import io.github.carlosarraes.ramo.security.PairingStore
import io.github.carlosarraes.ramo.uniffi.MobileReviewFileKind
import io.github.carlosarraes.ramo.uniffi.MobileReviewMap
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

interface ReviewMapRepository {
    suspend fun exact(repository: String, number: Long): ReviewMapUi
    suspend fun resolve(request: ReviewMapResolveRequest): ReviewMapServerResult?
    suspend fun poll(jobId: String): ReviewMapServerResult
    suspend fun retry(jobId: String): ReviewMapServerResult
    fun isPaired(): Boolean
}

class NativeReviewMapRepository(
    private val authenticator: NativeAuthenticator,
    private val pairingStore: PairingStore,
) : ReviewMapRepository {
    private var client: ReviewMapServerClient? = null

    override suspend fun exact(repository: String, number: Long): ReviewMapUi = withContext(Dispatchers.IO) {
        val session = checkNotNull(authenticator.session()) { "GitHub session expired" }
        session.reviewMap(repository, number.toULong()).toUi()
    }

    override suspend fun resolve(request: ReviewMapResolveRequest): ReviewMapServerResult? {
        val pairing = pairingStore.read() ?: return null
        return ReviewMapServerClient(pairing).also { client = it }.resolve(request)
    }

    override suspend fun poll(jobId: String): ReviewMapServerResult =
        checkNotNull(client) { "Review Map job was not started" }.poll(jobId)

    override suspend fun retry(jobId: String): ReviewMapServerResult =
        checkNotNull(client) { "Review Map job was not started" }.retry(jobId)

    override fun isPaired(): Boolean = pairingStore.read() != null
}

private fun MobileReviewMap.toUi() = ReviewMapUi(
    repository, number.toLong(), baseSha, headSha, additions.toLong(), deletions.toLong(),
    groups.map { group ->
        ReviewMapGroupUi(
            group.id, group.label, group.kind.toUi(), group.fileIds,
            group.additions.toLong(), group.deletions.toLong(), group.collapsedByDefault,
            group.summary, group.risk, group.reviewPriority?.toInt(),
        )
    },
    files.map { file ->
        ReviewMapFileUi(
            file.id, file.path, file.additions.toLong(), file.deletions.toLong(), file.kind.toUi(),
            file.owner, file.summary, file.risk, file.recommendedOrder?.toInt(), file.viewed,
        )
    },
    analysisModel,
)

private fun MobileReviewFileKind.toUi() = when (this) {
    MobileReviewFileKind.AUTHORED -> ReviewFileKindUi.Authored
    MobileReviewFileKind.TEST -> ReviewFileKindUi.Test
    MobileReviewFileKind.GENERATED -> ReviewFileKindUi.Generated
    MobileReviewFileKind.MIGRATION -> ReviewFileKindUi.Migration
    MobileReviewFileKind.DOCUMENTATION -> ReviewFileKindUi.Documentation
    MobileReviewFileKind.OTHER -> ReviewFileKindUi.Other
}
