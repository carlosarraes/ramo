package io.github.carlosarraes.ramo

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.enableEdgeToEdge
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import io.github.carlosarraes.ramo.auth.AuthState
import io.github.carlosarraes.ramo.auth.AuthViewModel
import io.github.carlosarraes.ramo.auth.NativeAuthenticator
import io.github.carlosarraes.ramo.auth.TokenScreen
import io.github.carlosarraes.ramo.inbox.InboxScreen
import io.github.carlosarraes.ramo.inbox.InboxViewModel
import io.github.carlosarraes.ramo.inbox.NativeInboxRepository
import io.github.carlosarraes.ramo.inbox.SecureInboxCache
import io.github.carlosarraes.ramo.notifications.NotificationScheduler
import io.github.carlosarraes.ramo.network.BootstrapStatus
import io.github.carlosarraes.ramo.network.NativeNetworkBootstrap
import io.github.carlosarraes.ramo.navigation.AppDestination
import io.github.carlosarraes.ramo.review.NativeReviewRepository
import io.github.carlosarraes.ramo.review.ReviewPreferencesStore
import io.github.carlosarraes.ramo.review.ReviewScreen
import io.github.carlosarraes.ramo.review.ReviewViewModel
import io.github.carlosarraes.ramo.review.SecureDraftStore
import io.github.carlosarraes.ramo.security.SecureTokenStore
import io.github.carlosarraes.ramo.ui.theme.RamoAppSurface

class MainActivity : ComponentActivity() {
    private val authenticator = NativeAuthenticator()
    private var destination by mutableStateOf<AppDestination>(AppDestination.Inbox)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        destination = intent.pullRequest() ?: AppDestination.Inbox
        setContent {
            RamoAppSurface {
                if (NativeNetworkBootstrap.status != BootstrapStatus.Ready) {
                    Text(
                        "Ramo couldn't initialize secure networking. Restart the app and try again.",
                        modifier = Modifier.padding(24.dp),
                    )
                    return@RamoAppSurface
                }
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
                        LaunchedEffect(current.login) {
                            inbox.refresh()
                            NotificationScheduler.schedule(applicationContext)
                        }
                        when (val currentDestination = destination) {
                            is AppDestination.Review -> {
                            val review: ReviewViewModel = viewModel(
                                key = "${currentDestination.repository}#${currentDestination.number}",
                            ) {
                                ReviewViewModel(
                                    NativeReviewRepository(authenticator),
                                    currentDestination.repository,
                                    currentDestination.number,
                                    SecureDraftStore(applicationContext),
                                )
                            }
                            val reviewState by review.state.collectAsState()
                            ReviewScreen(
                                state = reviewState,
                                codeSize = ReviewPreferencesStore(applicationContext).codeSize,
                                onBack = { destination = AppDestination.Inbox },
                                onFileSheet = review::setFileSheet,
                                onSummaryExpanded = review::setSummaryExpanded,
                                onSelectFile = review::selectFile,
                                onPrevious = review::previousFile,
                                onNext = review::nextFile,
                                onLoadMore = review::loadMoreRows,
                                onLastRow = review::lastRowVisible,
                                onViewed = review::setViewed,
                                onHorizontalOffset = review::setHorizontalOffset,
                                onSelectLine = review::selectLine,
                                onOpenComment = review::openComment,
                                onClearSelection = review::clearSelection,
                                onExpand = review::expand,
                                onFinish = review::setFinishing,
                                onCancelEditor = review::cancelEditor,
                                onSaveDraft = review::saveDraft,
                                onOverallBody = review::setOverallBody,
                                onVerdict = review::setVerdict,
                                onDeleteDraft = review::deleteDraft,
                                onConfirmation = review::setConfirmation,
                                onPublish = review::publish,
                                onDismissSuccess = review::dismissSuccess,
                                onRefreshAfterAttention = review::refreshAfterAttention,
                                onUndoViewed = review::undoViewed,
                                onDismissNotice = review::dismissNotice,
                            )
                            }
                            AppDestination.Inbox -> InboxScreen(
                                login = current.login,
                                state = inboxState,
                                onSelect = inbox::select,
                                onQuery = inbox::setQuery,
                                onDismissFailure = inbox::dismissFailure,
                                onRefresh = inbox::refresh,
                                onLoadMore = inbox::loadMore,
                                onOpen = { destination = AppDestination.Review(it.repository, it.number) },
                                onSettings = { destination = AppDestination.Settings },
                                onSignOut = {
                                    inbox.clear()
                                    SecureDraftStore(applicationContext).clearAll()
                                    auth.signOut()
                                },
                            )
                            AppDestination.Settings -> Column(
                                Modifier
                                    .fillMaxSize()
                                    .windowInsetsPadding(WindowInsets.safeDrawing)
                                    .padding(20.dp),
                            ) {
                                TextButton(onClick = { destination = AppDestination.Inbox }) { Text("Back") }
                                Text("Settings", style = MaterialTheme.typography.headlineSmall)
                                Text("@${current.login}", color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                        }
                    }
                    else -> TokenScreen(current, auth::validate, auth::retry, auth::signOut)
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        intent.pullRequest()?.let { destination = it }
    }

    private fun Intent.pullRequest(): AppDestination.Review? {
        val repository = getStringExtra(EXTRA_REPOSITORY) ?: return null
        val number = getLongExtra(EXTRA_NUMBER, -1).takeIf { it > 0 } ?: return null
        return AppDestination.Review(repository, number)
    }

    companion object {
        const val EXTRA_REPOSITORY = "repository"
        const val EXTRA_NUMBER = "number"
    }
}
