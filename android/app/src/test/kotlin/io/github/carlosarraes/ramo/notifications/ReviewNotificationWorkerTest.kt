package io.github.carlosarraes.ramo.notifications

import io.github.carlosarraes.ramo.security.TokenStore
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class ReviewNotificationWorkerTest {
    @Test fun noTokenDoesNoWork() = runTest {
        val fixture = fixture(token = null)
        assertEquals(WorkerOutcome.Success, fixture.runner.run())
        assertEquals(0, fixture.polls)
    }

    @Test fun notModifiedDoesNotPostOrRewriteCursor() = runTest {
        val fixture = fixture(page = NotificationPoll(emptyList(), "next", null, true))
        assertEquals(WorkerOutcome.Success, fixture.runner.run())
        assertEquals(0, fixture.posts.size)
        assertEquals(0, fixture.cursor.writes)
    }

    @Test fun postsOnlyNewRequestsAndThenAdvancesCursor() = runTest {
        val old = ReviewAlert("old", "a/b", 1, "Old")
        val fresh = ReviewAlert("new", "a/b", 2, "New")
        val fixture = fixture(
            page = NotificationPoll(listOf(old, fresh), "next", "now", false),
            cursor = NotificationCursor("before", null, setOf("old"), initialized = true),
        )
        assertEquals(WorkerOutcome.Success, fixture.runner.run())
        assertEquals(listOf(fresh), fixture.posts)
        assertEquals(setOf("old", "new"), fixture.cursor.value.seenIds)
    }

    @Test fun revokedTokenIsClearedAndFailsWithoutAdvancingCursor() = runTest {
        val fixture = fixture(failure = PollFailure.Revoked)
        assertEquals(WorkerOutcome.Failure, fixture.runner.run())
        assertNull(fixture.tokens.token)
        assertEquals(0, fixture.cursor.writes)
    }

    @Test fun firstSuccessfulPollSeedsWithoutSpammingExistingRequests() = runTest {
        val fixture = fixture(page = NotificationPoll(listOf(ReviewAlert("old", "a/b", 1, "Old")), null, null, false))
        assertEquals(WorkerOutcome.Success, fixture.runner.run())
        assertEquals(emptyList(), fixture.posts)
        assertEquals(setOf("old"), fixture.cursor.value.seenIds)
    }

    @Test fun rateLimitAndNetworkFailuresRetry() = runTest {
        assertEquals(WorkerOutcome.Retry, fixture(failure = PollFailure.Retryable).runner.run())
    }

    @Test fun unavailableOrganizationAccessDoesNotClearToken() = runTest {
        val fixture = fixture(failure = PollFailure.AccessUnavailable)

        assertEquals(WorkerOutcome.Failure, fixture.runner.run())
        assertEquals("token", fixture.tokens.token)
    }

    @Test fun unknownPollingFailureRetriesWithoutClearingToken() = runTest {
        val fixture = fixture(failure = IllegalStateException("event loop thread panicked"))

        assertEquals(WorkerOutcome.Retry, fixture.runner.run())
        assertEquals("token", fixture.tokens.token)
    }
}

private class Fixture(
    val tokens: MemoryTokens,
    val cursor: MemoryCursor,
    val posts: MutableList<ReviewAlert>,
    val runner: ReviewNotificationRunner,
    private val counter: Counter,
) { val polls: Int get() = counter.value }

private fun fixture(
    token: String? = "token",
    page: NotificationPoll = NotificationPoll(emptyList(), null, null, false),
    cursor: NotificationCursor = NotificationCursor(),
    failure: Throwable? = null,
): Fixture {
    val tokens = MemoryTokens(token)
    val cursors = MemoryCursor(cursor)
    val posts = mutableListOf<ReviewAlert>()
    val counter = Counter()
    val runner = ReviewNotificationRunner(tokens, cursors, ReviewPollerFactory {
        object : ReviewPoller {
            override suspend fun poll(cursor: NotificationCursor): NotificationPoll {
                counter.value += 1
                failure?.let { throw it }
                return page
            }
        }
    }, posts::add)
    return Fixture(tokens, cursors, posts, runner, counter)
}

private class Counter(var value: Int = 0)

private class MemoryTokens(var token: String?) : TokenStore {
    override fun read() = token
    override fun write(token: String) { this.token = token }
    override fun clear() { token = null }
}
private class MemoryCursor(var value: NotificationCursor) : NotificationCursorStore {
    var writes = 0
    override fun read() = value
    override fun write(cursor: NotificationCursor) { value = cursor; writes += 1 }
}
