package io.github.carlosarraes.ramo.inbox

import io.github.carlosarraes.ramo.auth.NativeAuthenticator
import io.github.carlosarraes.ramo.uniffi.MobileInboxKind
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

interface InboxRepository {
    suspend fun load(tab: InboxTab, after: String?): InboxPage
}

class NativeInboxRepository(
    private val authenticator: NativeAuthenticator,
    private val dispatcher: CoroutineDispatcher = Dispatchers.IO,
) : InboxRepository {
    override suspend fun load(tab: InboxTab, after: String?): InboxPage = withContext(dispatcher) {
        val session = checkNotNull(authenticator.session()) { "GitHub session expired" }
        session.inbox(
            if (tab == InboxTab.ReviewRequests) MobileInboxKind.REVIEW_REQUESTS else MobileInboxKind.AUTHORED,
            after,
        ).let { page ->
            InboxPage(
                items = page.items.map {
                    InboxItem(
                        it.nodeId, it.repository, it.number.toLong(), it.title, it.url, it.authorLogin,
                        it.updatedAt, it.isDraft, it.additions.toLong(), it.deletions.toLong(),
                        it.changedFiles.toLong(),
                    )
                },
                cursor = page.endCursor,
                hasNextPage = page.hasNextPage,
                warnings = page.warnings,
            )
        }
    }
}
