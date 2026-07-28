package io.github.carlosarraes.ramo.auth

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import io.github.carlosarraes.ramo.errors.FailureKind
import io.github.carlosarraes.ramo.errors.UserFacingFailure
import io.github.carlosarraes.ramo.errors.toUserFacingFailure
import io.github.carlosarraes.ramo.security.TokenStore
import io.github.carlosarraes.ramo.uniffi.MobileSession
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

sealed interface AuthState {
    data object SignedOut : AuthState
    data object Validating : AuthState
    data class SignedIn(val login: String) : AuthState
    data class Error(val message: String) : AuthState
    data class Failure(
        val failure: UserFacingFailure,
        val tokenRetained: Boolean,
    ) : AuthState
}

interface Authenticator {
    suspend fun validate(token: String): String
    fun close()
}

class NativeAuthenticator(
    private val dispatcher: CoroutineDispatcher = Dispatchers.IO,
) : Authenticator {
    private var session: MobileSession? = null

    override suspend fun validate(token: String): String = withContext(dispatcher) {
        close()
        val next = MobileSession(token)
        try {
            next.viewer().login.also { session = next }
        } catch (error: Throwable) {
            next.close()
            throw error
        }
    }

    fun session(): MobileSession? = session

    override fun close() {
        session?.close()
        session = null
    }
}

class AuthViewModel(
    private val tokenStore: TokenStore,
    private val authenticator: Authenticator,
) : ViewModel() {
    private val mutableState = MutableStateFlow<AuthState>(AuthState.SignedOut)
    val state: StateFlow<AuthState> = mutableState.asStateFlow()

    fun restore() {
        val token = tokenStore.read() ?: return
        validate(token, persist = false)
    }

    fun validate(token: String) = validate(token, persist = true)

    fun retry() {
        val token = tokenStore.read() ?: return
        validate(token, persist = false)
    }

    private fun validate(token: String, persist: Boolean) {
        if (token.isBlank()) {
            mutableState.value = AuthState.Error("Paste a GitHub token first")
            return
        }
        mutableState.value = AuthState.Validating
        viewModelScope.launch {
            runCatching { authenticator.validate(token) }
                .onSuccess { login ->
                    if (persist) tokenStore.write(token)
                    mutableState.value = AuthState.SignedIn(login)
                }
                .onFailure { error ->
                    val failure = error.toUserFacingFailure("Could not sign in to GitHub")
                    when (failure.kind) {
                        FailureKind.InvalidCredentials -> tokenStore.clear()
                        FailureKind.AccessUnavailable -> tokenStore.write(token)
                        else -> Unit
                    }
                    mutableState.value = AuthState.Failure(
                        failure,
                        tokenRetained = tokenStore.read() != null,
                    )
                }
        }
    }

    fun signOut() {
        authenticator.close()
        tokenStore.clear()
        mutableState.value = AuthState.SignedOut
    }

    override fun onCleared() {
        authenticator.close()
    }
}
