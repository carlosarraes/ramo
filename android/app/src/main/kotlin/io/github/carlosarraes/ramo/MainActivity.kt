package io.github.carlosarraes.ramo

import android.content.Intent
import android.Manifest
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.compose.setContent
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
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
import io.github.carlosarraes.ramo.review.NativeReviewRepository
import io.github.carlosarraes.ramo.review.ReviewPreferencesStore
import io.github.carlosarraes.ramo.review.ReviewScreen
import io.github.carlosarraes.ramo.review.ReviewViewModel
import io.github.carlosarraes.ramo.review.SecureDraftStore
import io.github.carlosarraes.ramo.security.SecureTokenStore
import io.github.carlosarraes.ramo.ui.theme.RamoTheme

class MainActivity : ComponentActivity() {
    private val authenticator = NativeAuthenticator()
    private var requestedPull by mutableStateOf<Pair<String, Long>?>(null)
    private val notificationPermission = registerForActivityResult(ActivityResultContracts.RequestPermission()) { }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        requestedPull = intent.pullRequest()
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
                        LaunchedEffect(current.login) {
                            inbox.refresh()
                            NotificationScheduler.schedule(applicationContext)
                        }
                        val requested = requestedPull
                        if (requested != null) {
                            val review: ReviewViewModel = viewModel(key = "${requested.first}#${requested.second}") {
                                ReviewViewModel(
                                    NativeReviewRepository(authenticator),
                                    requested.first,
                                    requested.second,
                                    SecureDraftStore(applicationContext),
                                )
                            }
                            val reviewState by review.state.collectAsState()
                            ReviewScreen(
                                state = reviewState,
                                codeSize = ReviewPreferencesStore(applicationContext).codeSize,
                                onBack = { requestedPull = null },
                                onDrawer = review::setDrawer,
                                onSelectFile = review::selectFile,
                                onPrevious = review::previousFile,
                                onNext = review::nextFile,
                                onLoadMore = review::loadMoreRows,
                                onLastRow = review::lastRowVisible,
                                onViewed = review::setViewed,
                                onHorizontalOffset = review::setHorizontalOffset,
                                onComment = review::beginComment,
                                onExpand = review::expand,
                                onFinish = review::setFinishing,
                                onExtendSelection = review::extendSelection,
                                onCancelEditor = review::cancelEditor,
                                onSaveDraft = review::saveDraft,
                                onOverallBody = review::setOverallBody,
                                onVerdict = review::setVerdict,
                                onDeleteDraft = review::deleteDraft,
                                onConfirmation = review::setConfirmation,
                                onPublish = review::publish,
                                onDismissSuccess = review::dismissSuccess,
                                onRefreshAfterAttention = review::refreshAfterAttention,
                            )
                        } else InboxScreen(
                            login = current.login,
                            state = inboxState,
                            onSelect = inbox::select,
                            onRefresh = inbox::refresh,
                            onLoadMore = inbox::loadMore,
                            onOpen = { requestedPull = it.repository to it.number },
                            onSignOut = {
                                inbox.clear()
                                SecureDraftStore(applicationContext).clearAll()
                                auth.signOut()
                            },
                            onEnableNotifications = {
                                if (Build.VERSION.SDK_INT >= 33) notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
                            },
                        )
                    }
                    else -> TokenScreen(current, auth::validate)
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        requestedPull = intent.pullRequest()
    }

    private fun Intent.pullRequest(): Pair<String, Long>? {
        val repository = getStringExtra(EXTRA_REPOSITORY) ?: return null
        val number = getLongExtra(EXTRA_NUMBER, -1).takeIf { it > 0 } ?: return null
        return repository to number
    }

    companion object {
        const val EXTRA_REPOSITORY = "repository"
        const val EXTRA_NUMBER = "number"
    }
}
