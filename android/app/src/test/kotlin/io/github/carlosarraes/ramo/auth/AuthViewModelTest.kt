package io.github.carlosarraes.ramo.auth

import io.github.carlosarraes.ramo.security.TokenStore
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

@OptIn(ExperimentalCoroutinesApi::class)
class AuthViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @BeforeTest fun setUp() = Dispatchers.setMain(dispatcher)
    @AfterTest fun tearDown() = Dispatchers.resetMain()

    @Test fun savesOnlyAfterValidationSucceeds() = runTest(dispatcher) {
        val store = MemoryTokenStore()
        val model = AuthViewModel(store, FakeAuthenticator(Result.success("carraes")))
        model.validate("github_pat_secret")
        assertEquals(AuthState.Validating, model.state.value)
        advanceUntilIdle()
        assertEquals(AuthState.SignedIn("carraes"), model.state.value)
        assertEquals("github_pat_secret", store.token)
    }

    @Test fun failedValidationDoesNotPersistToken() = runTest(dispatcher) {
        val store = MemoryTokenStore()
        val model = AuthViewModel(store, FakeAuthenticator(Result.failure(Exception("bad token"))))
        model.validate("bad")
        advanceUntilIdle()
        assertEquals(AuthState.Error("GitHub rejected this token"), model.state.value)
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
