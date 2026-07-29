package io.github.carlosarraes.ramo.review

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.github.carlosarraes.ramo.ui.theme.Green
import io.github.carlosarraes.ramo.ui.theme.Red
import io.github.carlosarraes.ramo.ui.components.FailureBanner
import kotlinx.coroutines.flow.distinctUntilChanged

@Composable
fun ReviewScreen(
    state: ReviewUiState,
    codeSize: Int,
    onBack: () -> Unit,
    onRetry: () -> Unit,
    onDismissError: () -> Unit,
    onFileSheet: (Boolean) -> Unit,
    onSummaryExpanded: (Boolean) -> Unit,
    onSelectFile: (Int) -> Unit,
    onPrevious: () -> Unit,
    onNext: () -> Unit,
    onLoadMore: () -> Unit,
    onLastRow: () -> Unit,
    onViewed: (Boolean) -> Unit,
    onHorizontalOffset: (Int) -> Unit,
    onSelectLine: (DiffRowUi) -> Unit,
    onOpenComment: () -> Unit,
    onClearSelection: () -> Unit,
    onExpand: (DiffRowUi) -> Unit,
    onFinish: (Boolean) -> Unit,
    onCancelEditor: () -> Unit,
    onSaveDraft: (String) -> Unit,
    onOverallBody: (String) -> Unit,
    onVerdict: (ReviewVerdictUi) -> Unit,
    onDeleteDraft: (String) -> Unit,
    onConfirmation: (Boolean) -> Unit,
    onPublish: () -> Unit,
    onDismissSuccess: () -> Unit,
    onRefreshAfterAttention: () -> Unit,
    onUndoViewed: () -> Unit,
    onDismissNotice: () -> Unit,
) {
    val pull = state.pullRequest
    val screen = state.screen
    if (pull == null || screen == null) {
        ReviewUnavailable(state, onBack, onRetry)
        return
    }

    val listState = rememberLazyListState()
    val horizontal = rememberScrollState(state.horizontalOffsets[state.selectedFile] ?: 0)
    val snackbarHostState = remember { SnackbarHostState() }
    LaunchedEffect(state.notice?.id) {
        val notice = state.notice ?: return@LaunchedEffect
        val result = snackbarHostState.showSnackbar(
            message = notice.message,
            actionLabel = if (notice.undoViewedFile != null) "Undo" else null,
            withDismissAction = true,
        )
        if (result == SnackbarResult.ActionPerformed) onUndoViewed()
        onDismissNotice()
    }
    LaunchedEffect(state.selectedFile) {
        horizontal.scrollTo(state.horizontalOffsets[state.selectedFile] ?: 0)
        listState.scrollToItem(0)
    }
    LaunchedEffect(horizontal) {
        snapshotFlow { horizontal.value }.distinctUntilChanged().collect(onHorizontalOffset)
    }
    LaunchedEffect(listState, screen.rows.size) {
        snapshotFlow { listState.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: 0 }
            .distinctUntilChanged()
            .collect { index ->
                if (index >= screen.rows.size - 40) onLoadMore()
                if (index >= screen.rows.size - 1) onLastRow()
            }
    }

    Scaffold(
        contentWindowInsets = WindowInsets.safeDrawing,
        snackbarHost = { SnackbarHost(snackbarHostState) },
        topBar = {
            ReviewTopBar(
                fileName = screen.file.path,
                currentFile = state.selectedFile + 1,
                fileCount = pull.files.size,
                onBack = onBack,
                onFiles = { onFileSheet(true) },
            )
        },
        bottomBar = {
            ReviewBottomNavigation(
                canPrevious = state.selectedFile > 0,
                canNext = state.selectedFile < pull.files.lastIndex,
                onPrevious = onPrevious,
                onFinish = { onFinish(true) },
                onNext = onNext,
            )
        },
        floatingActionButton = {
            if (state.selection != null && state.editor == null) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    TextButton(onClick = onClearSelection) { Text("Clear") }
                    ExtendedFloatingActionButton(
                        onClick = onOpenComment,
                        modifier = Modifier.testTag("comment-selection"),
                    ) {
                        Text("Comment")
                    }
                }
            }
        },
    ) { contentPadding ->
        Column(Modifier.fillMaxSize().padding(contentPadding)) {
            state.error?.let { error ->
                FailureBanner(
                    message = error,
                    retryable = true,
                    onRetry = onRetry,
                    onDismiss = onDismissError,
                )
            }
            LazyColumn(
                modifier = Modifier.fillMaxWidth().weight(1f).testTag("diff-list"),
                state = listState,
            ) {
            item {
                ReviewSummary(
                    pull = pull,
                    screen = screen,
                    expanded = state.summaryExpanded,
                    onExpanded = onSummaryExpanded,
                )
            }
            state.success?.let { message ->
                item {
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(message, Modifier.weight(1f), color = Green)
                        TextButton(onClick = onDismissSuccess) { Text("Dismiss") }
                    }
                }
            }
            item {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text("Viewed", Modifier.weight(1f), color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Checkbox(checked = screen.file.viewed, onCheckedChange = onViewed)
                }
            }
            items(screen.rows, key = DiffRowUi::key) { row ->
                DiffRow(
                    row = row,
                    horizontalScroll = horizontal,
                    codeSize = codeSize,
                    selected = row.isSelected(state.selection),
                    onSelect = onSelectLine,
                    onExpand = onExpand,
                )
                screen.threads.filter { thread ->
                    !thread.outdated && thread.endLine != null &&
                        (thread.endLine == row.newLine || thread.endLine == row.oldLine)
                }.forEach { ConversationCard(it) }
            }
            val previous = screen.threads.filter { it.outdated || it.endLine == null }
            if (previous.isNotEmpty()) {
                item { Text("Previous conversations", Modifier.padding(12.dp), fontWeight = FontWeight.Bold) }
                items(previous, key = ReviewThreadUi::id) { ConversationCard(it) }
            }
            }
        }
    }

    if (state.fileSheetOpen) {
        FileSheet(
            files = pull.files,
            selected = state.selectedFile,
            onSelect = onSelectFile,
            onDismiss = { onFileSheet(false) },
        )
    }
    if (state.needsAttention) NeedsAttentionSheet(state, onDeleteDraft, onRefreshAfterAttention)
    state.editor?.let { editor ->
        DraftEditor(
            editor,
            onSave = onSaveDraft,
            onCancel = onCancelEditor,
        )
    }
    if (state.finishing) {
        FinishReviewSheet(
            state,
            onDismiss = { onConfirmation(false); onFinish(false) },
            onBody = onOverallBody,
            onVerdict = onVerdict,
            onDeleteDraft = onDeleteDraft,
            onContinue = { onConfirmation(true) },
        )
    }
    if (state.confirmation) PublishConfirmation(state, onPublish, onDismiss = { onConfirmation(false) })
}

private fun DiffRowUi.isSelected(selection: LineSelectionUi?): Boolean {
    selection ?: return false
    val side = if (kind == LineKindUi.Deletion) CommentSideUi.Left else CommentSideUi.Right
    val line = if (side == CommentSideUi.Left) oldLine else newLine
    return side == selection.side && hunkIndex == selection.hunk &&
        line != null && line in selection.startLine..selection.endLine
}

@Composable
private fun ReviewUnavailable(state: ReviewUiState, onBack: () -> Unit, onRetry: () -> Unit) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            if (state.loading) {
                CircularProgressIndicator()
            } else {
                Text(state.error ?: "Pull request unavailable")
                Row {
                    TextButton(onClick = onBack) { Text("Back") }
                    TextButton(onClick = onRetry) { Text("Retry") }
                }
            }
        }
    }
}

@Composable
private fun ReviewSummary(
    pull: PullRequestUi,
    screen: FileScreenUi,
    expanded: Boolean,
    onExpanded: (Boolean) -> Unit,
) {
    val percent = if (screen.fileCount == 0) 100 else screen.viewedCount * 100 / screen.fileCount
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clickable { onExpanded(!expanded) }
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            Text("${pull.repository} #${pull.number}", fontWeight = FontWeight.SemiBold)
            Text(if (expanded) "Hide details" else "Details", color = MaterialTheme.colorScheme.primary)
        }
        if (expanded) {
            Text(pull.title, style = MaterialTheme.typography.titleMedium)
            Text("${pull.headRef} → ${pull.baseRef}", color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Text("+${pull.additions}", color = Green)
            Text("−${pull.deletions}", color = Red)
            Text("$percent% viewed", color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}
