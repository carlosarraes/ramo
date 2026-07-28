package io.github.carlosarraes.ramo.errors

import io.github.carlosarraes.ramo.uniffi.MobileException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class UserFacingFailureTest {
    @Test
    fun forbiddenExplainsApprovalWithoutClaimingItIsPending() {
        val failure = MobileException.AccessUnavailable()
            .toUserFacingFailure("Could not load")

        assertEquals(FailureKind.AccessUnavailable, failure.kind)
        assertEquals(
            "Organization access isn't active. This token may still be awaiting approval.",
            failure.message,
        )
        assertTrue(failure.retryable)
    }

    @Test
    fun unknownFailureNeverLeaksItsMessage() {
        val failure = IllegalStateException("event loop thread panicked")
            .toUserFacingFailure("Could not sign in to GitHub")

        assertEquals("Could not sign in to GitHub", failure.message)
        assertFalse(failure.message.contains("panicked"))
    }
}
