package io.github.carlosarraes.ramo.notifications

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationCompat
import androidx.work.CoroutineWorker
import androidx.work.WorkerParameters
import io.github.carlosarraes.ramo.MainActivity
import io.github.carlosarraes.ramo.security.SecureTokenStore
import io.github.carlosarraes.ramo.security.TokenStore
import io.github.carlosarraes.ramo.uniffi.MobileException
import io.github.carlosarraes.ramo.uniffi.MobileSession
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

data class ReviewAlert(val id: String, val repository: String, val number: Long, val title: String)
data class NotificationPoll(val alerts: List<ReviewAlert>, val etag: String?, val lastModified: String?, val notModified: Boolean)

sealed class PollFailure : Exception() {
    data object Revoked : PollFailure()
    data object Retryable : PollFailure()
    data object Fatal : PollFailure()
}

interface ReviewPoller { suspend fun poll(cursor: NotificationCursor): NotificationPoll }
fun interface ReviewPollerFactory { fun create(token: String): ReviewPoller }
fun interface ReviewAlertPoster { fun post(alert: ReviewAlert) }

enum class WorkerOutcome { Success, Retry, Failure }

class ReviewNotificationRunner(
    private val tokens: TokenStore,
    private val cursors: NotificationCursorStore,
    private val pollers: ReviewPollerFactory,
    private val poster: ReviewAlertPoster,
) {
    suspend fun run(): WorkerOutcome {
        val token = tokens.read() ?: return WorkerOutcome.Success
        val cursor = cursors.read()
        return try {
            val page = pollers.create(token).poll(cursor)
            if (!page.notModified) {
                if (cursor.initialized) {
                    page.alerts.filterNot { it.id in cursor.seenIds }.forEach(poster::post)
                }
                cursors.write(
                    NotificationCursor(
                        page.etag,
                        page.lastModified,
                        cursor.seenIds + page.alerts.map(ReviewAlert::id),
                        initialized = true,
                    ),
                )
            }
            WorkerOutcome.Success
        } catch (_: PollFailure.Revoked) {
            tokens.clear()
            WorkerOutcome.Failure
        } catch (_: PollFailure.Fatal) {
            WorkerOutcome.Failure
        } catch (_: PollFailure.Retryable) {
            WorkerOutcome.Retry
        }
    }
}

class BridgeReviewPoller(private val token: String) : ReviewPoller {
    override suspend fun poll(cursor: NotificationCursor): NotificationPoll = withContext(Dispatchers.IO) {
        val session = try {
            MobileSession(token)
        } catch (_: MobileException.InvalidCredentials) {
            throw PollFailure.Revoked
        }
        try {
            session.reviewNotifications(cursor.etag, cursor.lastModified).let { page ->
                NotificationPoll(
                    page.notifications.map { ReviewAlert(it.id, it.repository, it.number.toLong(), it.title) },
                    page.etag,
                    page.lastModified,
                    page.notModified,
                )
            }
        } catch (_: MobileException.InvalidCredentials) {
            throw PollFailure.Revoked
        } catch (_: MobileException.AccessUnavailable) {
            throw PollFailure.Fatal
        } catch (_: MobileException.Network) {
            throw PollFailure.Retryable
        } catch (_: MobileException.RateLimited) {
            throw PollFailure.Retryable
        } catch (_: MobileException.Unexpected) {
            throw PollFailure.Retryable
        } finally {
            session.close()
        }
    }
}

class AndroidReviewAlertPoster(private val context: Context) : ReviewAlertPoster {
    override fun post(alert: ReviewAlert) {
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL_ID, "Review requests", NotificationManager.IMPORTANCE_DEFAULT),
        )
        val intent = Intent(context, MainActivity::class.java)
            .putExtra(MainActivity.EXTRA_REPOSITORY, alert.repository)
            .putExtra(MainActivity.EXTRA_NUMBER, alert.number)
            .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        val pending = PendingIntent.getActivity(
            context,
            alert.id.hashCode(),
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        manager.notify(
            alert.id.hashCode(),
            NotificationCompat.Builder(context, CHANNEL_ID)
                .setSmallIcon(android.R.drawable.stat_notify_more)
                .setContentTitle("${alert.repository} #${alert.number}")
                .setContentText(alert.title)
                .setContentIntent(pending)
                .setAutoCancel(true)
                .build(),
        )
    }

    companion object { const val CHANNEL_ID = "review_requests" }
}

class ReviewNotificationWorker(context: Context, parameters: WorkerParameters) : CoroutineWorker(context, parameters) {
    override suspend fun doWork(): Result {
        val runner = ReviewNotificationRunner(
            SecureTokenStore(applicationContext),
            PreferencesNotificationCursorStore(applicationContext),
            ReviewPollerFactory(::BridgeReviewPoller),
            AndroidReviewAlertPoster(applicationContext),
        )
        return when (runner.run()) {
            WorkerOutcome.Success -> Result.success()
            WorkerOutcome.Retry -> Result.retry()
            WorkerOutcome.Failure -> Result.failure()
        }
    }
}
