package io.github.carlosarraes.ramo.notifications

import android.content.Context

data class NotificationCursor(
    val etag: String? = null,
    val lastModified: String? = null,
    val seenIds: Set<String> = emptySet(),
    val initialized: Boolean = false,
)

interface NotificationCursorStore {
    fun read(): NotificationCursor
    fun write(cursor: NotificationCursor)
}

class PreferencesNotificationCursorStore(context: Context) : NotificationCursorStore {
    private val preferences = context.getSharedPreferences("review-notification-cursor", Context.MODE_PRIVATE)

    override fun read() = NotificationCursor(
        preferences.getString("etag", null),
        preferences.getString("last-modified", null),
        preferences.getStringSet("seen", emptySet()).orEmpty(),
        preferences.getBoolean("initialized", false),
    )

    override fun write(cursor: NotificationCursor) {
        preferences.edit()
            .putString("etag", cursor.etag)
            .putString("last-modified", cursor.lastModified)
            .putStringSet("seen", cursor.seenIds.toList().takeLast(MAX_SEEN).toSet())
            .putBoolean("initialized", cursor.initialized)
            .apply()
    }

    private companion object { const val MAX_SEEN = 100 }
}
