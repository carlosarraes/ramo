package io.github.carlosarraes.ramo.inbox

import android.content.Context
import io.github.carlosarraes.ramo.security.EncryptedBlobStore
import io.github.carlosarraes.ramo.uniffi.MobileInboxCache
import io.github.carlosarraes.ramo.uniffi.MobileInboxPage
import io.github.carlosarraes.ramo.uniffi.MobilePullRequest
import io.github.carlosarraes.ramo.uniffi.decodeInboxCache
import io.github.carlosarraes.ramo.uniffi.encodeInboxCache

interface InboxCache {
    fun load(): Pair<InboxPage, InboxPage>?
    fun save(reviewRequests: InboxPage, authored: InboxPage)
    fun clear()
}

class SecureInboxCache(context: Context) : InboxCache {
    private val blobs = EncryptedBlobStore(context, "ramo.mobile.inbox.v1", "inbox.enc")

    override fun load(): Pair<InboxPage, InboxPage>? = runCatching {
        val value = blobs.read()?.toString(Charsets.UTF_8) ?: return null
        decodeInboxCache(value).let { it.reviewRequests.toLocal() to it.authored.toLocal() }
    }.getOrNull()

    override fun save(reviewRequests: InboxPage, authored: InboxPage) {
        val encoded = encodeInboxCache(MobileInboxCache(reviewRequests.toMobile(), authored.toMobile()))
        blobs.write(encoded.toByteArray(Charsets.UTF_8))
    }

    override fun clear() = blobs.clear()
}

private fun InboxPage.toMobile() = MobileInboxPage(
    items.map { MobilePullRequest(
        it.nodeId, it.repository, it.number.toULong(), it.title, it.url, it.author, it.updatedAt,
        it.draft, it.additions.toULong(), it.deletions.toULong(), it.changedFiles.toULong(),
    ) },
    cursor,
    hasNextPage,
    warnings,
)

private fun MobileInboxPage.toLocal() = InboxPage(
    items.map { InboxItem(
        it.nodeId, it.repository, it.number.toLong(), it.title, it.url, it.authorLogin, it.updatedAt,
        it.isDraft, it.additions.toLong(), it.deletions.toLong(), it.changedFiles.toLong(),
    ) },
    endCursor,
    hasNextPage,
    warnings,
)
