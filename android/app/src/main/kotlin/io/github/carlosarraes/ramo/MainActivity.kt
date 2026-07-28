package io.github.carlosarraes.ramo

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.lifecycle.viewmodel.compose.viewModel
import io.github.carlosarraes.ramo.auth.AuthState
import io.github.carlosarraes.ramo.auth.AuthViewModel
import io.github.carlosarraes.ramo.auth.NativeAuthenticator
import io.github.carlosarraes.ramo.auth.TokenScreen
import io.github.carlosarraes.ramo.inbox.InboxScreen
import io.github.carlosarraes.ramo.inbox.InboxViewModel
import io.github.carlosarraes.ramo.inbox.NativeInboxRepository
import io.github.carlosarraes.ramo.inbox.SecureInboxCache
import io.github.carlosarraes.ramo.security.SecureTokenStore
import io.github.carlosarraes.ramo.ui.theme.RamoTheme

class MainActivity : ComponentActivity() {
    private val authenticator = NativeAuthenticator()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            RamoTheme {
                val auth: AuthViewModel = viewModel {
                    AuthViewModel(SecureTokenStore(applicationContext), authenticator)
                }
                val state by auth.state.collectAsState()
                LaunchedEffect(Unit) { auth.restore() }
                when (val current = state) {
                    is AuthState.SignedIn -> {
                        val inbox: InboxViewModel = viewModel {
                            InboxViewModel(
                                NativeInboxRepository(authenticator),
                                SecureInboxCache(applicationContext),
                            )
                        }
                        val inboxState by inbox.state.collectAsState()
                        LaunchedEffect(current.login) { inbox.refresh() }
                        InboxScreen(
                            login = current.login,
                            state = inboxState,
                            onSelect = inbox::select,
                            onRefresh = inbox::refresh,
                            onLoadMore = inbox::loadMore,
                            onOpen = { startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(it.url))) },
                            onSignOut = {
                                inbox.clear()
                                auth.signOut()
                            },
                        )
                    }
                    else -> TokenScreen(current, auth::validate)
                }
            }
        }
    }
}
