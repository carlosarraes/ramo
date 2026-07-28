package io.github.carlosarraes.ramo.errors

import android.util.Log
import io.github.carlosarraes.ramo.uniffi.MobileException

enum class FailureKind {
    InvalidCredentials,
    AccessUnavailable,
    RateLimited,
    Network,
    StaleRevision,
    Validation,
    Unexpected,
}

data class UserFacingFailure(
    val kind: FailureKind,
    val message: String,
    val retryable: Boolean,
)

fun Throwable.toUserFacingFailure(fallback: String): UserFacingFailure {
    val failure = when (this) {
        is MobileException.InvalidCredentials -> UserFacingFailure(
            FailureKind.InvalidCredentials,
            "GitHub rejected this token",
            retryable = false,
        )
        is MobileException.AccessUnavailable -> UserFacingFailure(
            FailureKind.AccessUnavailable,
            "Organization access isn't active. This token may still be awaiting approval.",
            retryable = true,
        )
        is MobileException.RateLimited -> UserFacingFailure(
            FailureKind.RateLimited,
            "GitHub rate limit exceeded; try again later",
            retryable = true,
        )
        is MobileException.Network -> UserFacingFailure(
            FailureKind.Network,
            "Could not reach GitHub",
            retryable = true,
        )
        is MobileException.StaleRevision -> UserFacingFailure(
            FailureKind.StaleRevision,
            "The pull request changed while you were reviewing",
            retryable = true,
        )
        is MobileException.Validation -> UserFacingFailure(
            FailureKind.Validation,
            "GitHub rejected this operation",
            retryable = false,
        )
        is MobileException.Unexpected -> UserFacingFailure(
            FailureKind.Unexpected,
            "GitHub returned an unexpected response",
            retryable = true,
        )
        else -> UserFacingFailure(FailureKind.Unexpected, fallback, retryable = true)
    }
    if (this !is MobileException) {
        runCatching { Log.e("Ramo", "Unexpected application failure", this) }
    }
    return failure
}
