package io.github.carlosarraes.ramo.auth

import io.github.carlosarraes.ramo.errors.FailureKind
import io.github.carlosarraes.ramo.security.TokenStore
import io.github.carlosarraes.ramo.uniffi.MobileException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

@OptIn(ExperimentalCoroutinesApi::class)
class AuthViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @BeforeTest fun setUp() = Dispatchers.setMain(dispatcher)
    @AfterTest fun tearDown() = Dispatchers.resetMain()

    @Test fun savesOnlyAfterValidationSucceeds() = runTest(dispatcher) {
        val store = MemoryTokenStore()
        val model = AuthViewModel(store, FakeAuthenticator(Result.success("carraes")))
        model.validate("candidate-token")
        assertEquals(AuthState.Validating, model.state.value)
        advanceUntilIdle()
        assertEquals(AuthState.SignedIn("carraes"), model.state.value)
        assertEquals("candidate-token", store.token)
    }

    @Test fun invalidValidationDoesNotPersistToken() = runTest(dispatcher) {
        val store = MemoryTokenStore()
        val model = AuthViewModel(
            store,
            FakeAuthenticator(Result.failure(MobileException.InvalidCredentials())),
        )
        model.validate("bad")
        advanceUntilIdle()
        assertEquals(
            FailureKind.InvalidCredentials,
            (model.state.value as AuthState.Failure).failure.kind,
        )
        assertNull(store.token)
    }

    @Test fun signOutClearsTokenAndSession() = runTest(dispatcher) {
        val store = MemoryTokenStore("saved")
        val authenticator = FakeAuthenticator(Result.success("carraes"))
        val model = AuthViewModel(store, authenticator)
        model.restore()
        advanceUntilIdle()
        model.signOut()
        assertEquals(AuthState.SignedOut, model.state.value)
        assertNull(store.token)
        assertEquals(1, authenticator.closed)
    }

    @Test fun restoredTokenCanRetryWithoutAnotherPaste() = runTest(dispatcher) {
        val store = MemoryTokenStore("saved")
        val authenticator = RetryAuthenticator()
        val model = AuthViewModel(store, authenticator)

        model.restore()
        advanceUntilIdle()
        assertTrue((model.state.value as AuthState.Failure).tokenRetained)

        model.retry()
        advanceUntilIdle()

        assertEquals(listOf("saved", "saved"), authenticator.tokens)
        assertEquals(AuthState.SignedIn("carraes"), model.state.value)
    }

    @Test fun forbiddenTokenIsRetainedForOrganizationApproval() = runTest(dispatcher) {
        val store = MemoryTokenStore()
        val model = AuthViewModel(
            store,
            FakeAuthenticator(Result.failure(MobileException.AccessUnavailable())),
        )

        model.validate("candidate-token")
        advanceUntilIdle()

        assertEquals("candidate-token", store.token)
        assertTrue((model.state.value as AuthState.Failure).tokenRetained)
    }

    @Test fun invalidRestoredTokenIsRemoved() = runTest(dispatcher) {
        val store = MemoryTokenStore("revoked")
        val model = AuthViewModel(
            store,
            FakeAuthenticator(Result.failure(MobileException.InvalidCredentials())),
        )

        model.restore()
        advanceUntilIdle()

        assertNull(store.token)
    }
}

private class MemoryTokenStore(var token: String? = null) : TokenStore {
    override fun read() = token
    override fun write(token: String) { this.token = token }
    override fun clear() { token = null }
}

private class FakeAuthenticator(private val answer: Result<String>) : Authenticator {
    var closed = 0
    override suspend fun validate(token: String) = answer.getOrThrow()
    override fun close() { closed += 1 }
}

private class RetryAuthenticator : Authenticator {
    val tokens = mutableListOf<String>()

    override suspend fun validate(token: String): String {
        tokens += token
        if (tokens.size == 1) throw MobileException.Network()
        return "carraes"
    }

    override fun close() = Unit
}
