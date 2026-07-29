package io.github.carlosarraes.ramo

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.enableEdgeToEdge
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.viewmodel.compose.viewModel
import io.github.carlosarraes.ramo.auth.AuthState
import io.github.carlosarraes.ramo.auth.AuthViewModel
import io.github.carlosarraes.ramo.auth.NativeAuthenticator
import io.github.carlosarraes.ramo.auth.TokenScreen
import io.github.carlosarraes.ramo.inbox.InboxScreen
import io.github.carlosarraes.ramo.inbox.InboxViewModel
import io.github.carlosarraes.ramo.inbox.NativeInboxRepository
import io.github.carlosarraes.ramo.inbox.SecureInboxCache
import io.github.carlosarraes.ramo.notifications.NotificationPermissionSheet
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
import io.github.carlosarraes.ramo.settings.SettingsScreen
import io.github.carlosarraes.ramo.ui.theme.RamoAppSurface

class MainActivity : ComponentActivity() {
    private val authenticator = NativeAuthenticator()
    private var destination by mutableStateOf<AppDestination>(AppDestination.Inbox)
    private var notificationsGranted by mutableStateOf(false)
    private var showNotificationPermission by mutableStateOf(false)
    private val notificationPermissionLauncher = registerForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted -> notificationsGranted = granted }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        notificationsGranted = notificationsAreGranted()
        destination = intent.pullRequest() ?: AppDestination.Inbox
        val preferences = ReviewPreferencesStore(applicationContext)
        setContent {
            RamoAppSurface {
                var codeSize by rememberSaveable { mutableIntStateOf(preferences.codeSize) }
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
                            if (!preferences.notificationPromptHandled && !notificationsGranted) {
                                showNotificationPermission = true
                            }
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
                                    codeSize = codeSize,
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
                            AppDestination.Settings -> SettingsScreen(
                                login = current.login,
                                codeSize = codeSize,
                                notificationsGranted = notificationsGranted,
                                onCodeSize = { value ->
                                    codeSize = value.coerceIn(11, 20)
                                    preferences.codeSize = codeSize
                                },
                                onEnableNotifications = ::requestNotificationPermission,
                                onBack = { destination = AppDestination.Inbox },
                                onSignOut = {
                                    inbox.clear()
                                    SecureDraftStore(applicationContext).clearAll()
                                    auth.signOut()
                                },
                            )
                        }
                        if (showNotificationPermission) {
                            NotificationPermissionSheet(
                                onEnable = {
                                    preferences.notificationPromptHandled = true
                                    showNotificationPermission = false
                                    requestNotificationPermission()
                                },
                                onNotNow = {
                                    preferences.notificationPromptHandled = true
                                    showNotificationPermission = false
                                },
                            )
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

    override fun onResume() {
        super.onResume()
        notificationsGranted = notificationsAreGranted()
    }

    private fun requestNotificationPermission() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            notificationPermissionLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
        } else {
            notificationsGranted = true
        }
    }

    private fun notificationsAreGranted() =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) ==
            PackageManager.PERMISSION_GRANTED

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
