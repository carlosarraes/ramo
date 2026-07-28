package io.github.carlosarraes.ramo.review

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.github.carlosarraes.ramo.ui.theme.Green
import io.github.carlosarraes.ramo.ui.theme.Red
import kotlinx.coroutines.flow.distinctUntilChanged

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ReviewScreen(
    state: ReviewUiState,
    codeSize: Int,
    onBack: () -> Unit,
    onDrawer: (Boolean) -> Unit,
    onSelectFile: (Int) -> Unit,
    onPrevious: () -> Unit,
    onNext: () -> Unit,
    onLoadMore: () -> Unit,
    onLastRow: () -> Unit,
    onViewed: (Boolean) -> Unit,
    onHorizontalOffset: (Int) -> Unit,
    onComment: (DiffRowUi) -> Unit,
    onExpand: (DiffRowUi) -> Unit,
    onFinish: (Boolean) -> Unit,
    onExtendSelection: (Boolean) -> Unit,
    onCancelEditor: () -> Unit,
    onSaveDraft: (String) -> Unit,
    onOverallBody: (String) -> Unit,
    onVerdict: (ReviewVerdictUi) -> Unit,
    onDeleteDraft: (String) -> Unit,
    onConfirmation: (Boolean) -> Unit,
    onPublish: () -> Unit,
    onDismissSuccess: () -> Unit,
    onRefreshAfterAttention: () -> Unit,
) {
    val pull = state.pullRequest
    val screen = state.screen
    if (pull == null || screen == null) {
        Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
            if (state.loading) CircularProgressIndicator() else Text(state.error ?: "Pull request unavailable")
            TextButton(onClick = onBack) { Text("Back") }
        }
        return
    }
    val drawerState = rememberDrawerState(DrawerValue.Closed)
    LaunchedEffect(state.drawerOpen) {
        if (state.drawerOpen) drawerState.open() else drawerState.close()
    }
    ModalNavigationDrawer(
        drawerState = drawerState,
        gesturesEnabled = false,
        drawerContent = { ModalDrawerSheet { FileDrawer(pull.files, state.selectedFile, onSelectFile) } },
    ) {
        Column(Modifier.fillMaxSize()) {
            ReviewSummary(pull, screen, onBack)
            state.success?.let { message ->
                Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(message, Modifier.weight(1f), color = io.github.carlosarraes.ramo.ui.theme.Green)
                    TextButton(onClick = onDismissSuccess) { Text("Dismiss") }
                }
            }
            Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                TextButton(onClick = { onDrawer(true) }) { Text("Files") }
                Text(screen.file.path, Modifier.weight(1f), fontWeight = FontWeight.SemiBold)
                Text("Viewed")
                Checkbox(checked = screen.file.viewed, onCheckedChange = onViewed)
            }
            val listState = rememberLazyListState()
            val horizontal = rememberScrollState(state.horizontalOffsets[state.selectedFile] ?: 0)
            LaunchedEffect(state.selectedFile) {
                horizontal.scrollTo(state.horizontalOffsets[state.selectedFile] ?: 0)
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
            LazyColumn(Modifier.weight(1f), state = listState) {
                items(screen.rows, key = DiffRowUi::key) { row ->
                    DiffRow(row, horizontal, codeSize, onComment, onExpand)
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
            state.error?.let { Text(it, Modifier.padding(horizontal = 12.dp), color = MaterialTheme.colorScheme.error) }
            if (state.needsAttention) NeedsAttentionSheet(state, onDeleteDraft, onRefreshAfterAttention)
            Row(Modifier.fillMaxWidth().padding(8.dp), horizontalArrangement = Arrangement.SpaceBetween) {
                TextButton(onClick = onPrevious, enabled = state.selectedFile > 0) { Text("Previous file") }
                Button(onClick = { onFinish(true) }) { Text("Finish review") }
                TextButton(onClick = onNext, enabled = state.selectedFile + 1 < pull.files.size) { Text("Next file") }
            }
        }
    }
    state.editor?.let { editor ->
        DraftEditor(
            editor,
            onExtendPrevious = { onExtendSelection(false) },
            onExtendNext = { onExtendSelection(true) },
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
    if (state.confirmation) {
        PublishConfirmation(state, onPublish, onDismiss = { onConfirmation(false) })
    }
}

@Composable
private fun ReviewSummary(pull: PullRequestUi, screen: FileScreenUi, onBack: () -> Unit) {
    val percent = if (screen.fileCount == 0) 100 else screen.viewedCount * 100 / screen.fileCount
    Column(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = onBack) { Text("Back") }
            Text("${pull.repository} #${pull.number}", fontWeight = FontWeight.Bold)
        }
        Text(pull.title, style = MaterialTheme.typography.titleMedium)
        Row {
            Text("+${pull.additions}", color = Green)
            Text("  −${pull.deletions}", color = Red)
            Text("   ${screen.fileIndex + 1} / ${screen.fileCount} files · $percent%")
        }
    }
}
