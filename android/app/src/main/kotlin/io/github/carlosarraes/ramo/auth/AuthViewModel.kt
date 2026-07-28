package io.github.carlosarraes.ramo.auth

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import io.github.carlosarraes.ramo.security.TokenStore
import io.github.carlosarraes.ramo.uniffi.MobileSession
import io.github.carlosarraes.ramo.uniffi.MobileException
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
                    mutableState.value = AuthState.Error(userMessage(error))
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

    private fun userMessage(error: Throwable): String {
        when (error) {
            is MobileException.InvalidCredentials -> return "GitHub rejected this token"
            is MobileException.Forbidden -> return "This token is missing a required permission"
            is MobileException.RateLimited -> return "GitHub rate limit exceeded; try again later"
            is MobileException.Network -> return "Could not reach GitHub"
            is MobileException.Unexpected -> return "GitHub returned an unexpected response"
        }
        val message = error.message.orEmpty()
        return when {
            message.contains("token", ignoreCase = true) -> "GitHub rejected this token"
            message.contains("reach", ignoreCase = true) -> "Could not reach GitHub"
            else -> message.ifBlank { "Could not sign in to GitHub" }
        }
    }
}
