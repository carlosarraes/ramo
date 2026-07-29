package io.github.carlosarraes.ramo.inbox

import android.content.Context
import io.github.carlosarraes.ramo.security.EncryptedBlobStore
import io.github.carlosarraes.ramo.uniffi.MobileInboxCache
import io.github.carlosarraes.ramo.uniffi.MobileInboxPage
import io.github.carlosarraes.ramo.uniffi.MobilePullRequest
import io.github.carlosarraes.ramo.uniffi.decodeInboxCache
import io.github.carlosarraes.ramo.uniffi.encodeInboxCache

interface InboxCache {
    fun load(): InboxCacheValue?
    fun save(value: InboxCacheValue)
    fun clear()
}

class SecureInboxCache(context: Context) : InboxCache {
    private val blobs = EncryptedBlobStore(context, "ramo.mobile.inbox.v1", "inbox.enc")
    private val metadata = context.getSharedPreferences("ramo.mobile.inbox.metadata.v1", Context.MODE_PRIVATE)

    override fun load(): InboxCacheValue? = runCatching {
        val value = blobs.read()?.toString(Charsets.UTF_8) ?: return null
        decodeInboxCache(value).let {
            InboxCacheValue(
                reviewRequests = it.reviewRequests.toLocal(),
                authored = it.authored.toLocal(),
                refreshedAtEpochMillis = metadata.getLong("refreshed-at", 0L),
            )
        }
    }.getOrNull()

    override fun save(value: InboxCacheValue) {
        val encoded = encodeInboxCache(
            MobileInboxCache(value.reviewRequests.toMobile(), value.authored.toMobile()),
        )
        blobs.write(encoded.toByteArray(Charsets.UTF_8))
        metadata.edit().putLong("refreshed-at", value.refreshedAtEpochMillis).apply()
    }

    override fun clear() {
        blobs.clear()
        metadata.edit().clear().apply()
    }
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
