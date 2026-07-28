package io.github.carlosarraes.ramo.review

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import io.github.carlosarraes.ramo.uniffi.MobileException

class ReviewViewModel(
    private val repository: ReviewRepository,
    private val repositoryName: String,
    private val number: Long,
    private val draftStore: ReviewDraftStore = NoopDraftStore(),
) : ViewModel() {
    private val mutableState = MutableStateFlow(ReviewUiState())
    val state: StateFlow<ReviewUiState> = mutableState.asStateFlow()
    private var pageJob: Job? = null

    init { open() }

    fun open() {
        mutableState.value = ReviewUiState(loading = true)
        viewModelScope.launch {
            runCatching { repository.open(repositoryName, number) }
                .onSuccess { pull ->
                    val stored = draftStore.load(repositoryName, number)
                    mutableState.value = mutableState.value.copy(
                        pullRequest = pull,
                        selectedFile = 0,
                        drafts = stored?.comments.orEmpty(),
                        overallBody = stored?.body.orEmpty(),
                        needsAttention = stored != null && stored.revision != pull.revision,
                    )
                    loadFile(0)
                }
                .onFailure { mutableState.value = mutableState.value.copy(loading = false, error = message(it)) }
        }
    }

    fun selectFile(index: Int) {
        val files = mutableState.value.pullRequest?.files.orEmpty()
        if (index !in files.indices || index == mutableState.value.selectedFile) return
        mutableState.value = mutableState.value.copy(selectedFile = index, drawerOpen = false)
        loadFile(index)
    }

    fun previousFile() = selectFile(mutableState.value.selectedFile - 1)
    fun nextFile() = selectFile(mutableState.value.selectedFile + 1)
    fun setDrawer(open: Boolean) { mutableState.value = mutableState.value.copy(drawerOpen = open) }
    fun setFinishing(open: Boolean) { mutableState.value = mutableState.value.copy(finishing = open) }

    fun beginComment(row: DiffRowUi) {
        if (!row.commentable) return
        val side = if (row.kind == LineKindUi.Deletion) CommentSideUi.Left else CommentSideUi.Right
        val line = if (side == CommentSideUi.Left) row.oldLine else row.newLine
        if (line != null) {
            mutableState.value = mutableState.value.copy(
                editor = DraftEditorUi(row.key, side, row.hunkIndex, line, line),
            )
        }
    }

    fun extendSelection(forward: Boolean) {
        val state = mutableState.value
        val editor = state.editor ?: return
        val rows = state.screen?.rows.orEmpty()
        val selected = rows.filter { row ->
            row.hunkIndex == editor.hunk && row.commentable && row.side() == editor.side
        }
        val target = if (forward) editor.endLine + 1 else editor.startLine - 1
        if (selected.any { it.lineFor(editor.side) == target }) {
            mutableState.value = state.copy(
                editor = if (forward) editor.copy(endLine = target) else editor.copy(startLine = target),
            )
        }
    }

    fun cancelEditor() { mutableState.value = mutableState.value.copy(editor = null) }

    fun saveDraft(body: String) {
        val state = mutableState.value
        val editor = state.editor ?: return
        val screen = state.screen ?: return
        val pull = state.pullRequest ?: return
        val hunkRows = screen.rows.filter { it.hunkIndex == editor.hunk && it.side() == editor.side }
        val selected = hunkRows.filter { (it.lineFor(editor.side) ?: -1) in editor.startLine..editor.endLine }
        if (selected.isEmpty()) return
        val firstIndex = hunkRows.indexOf(selected.first())
        val lastIndex = hunkRows.indexOf(selected.last())
        val input = DraftInputUi(
            pull.repository, pull.number, pull.revision, screen.file.path, editor.side,
            editor.startLine, editor.endLine, editor.hunk,
            hunkRows.subList((firstIndex - 2).coerceAtLeast(0), firstIndex).map(DiffRowUi::sourceText),
            selected.map(DiffRowUi::sourceText),
            hunkRows.subList(lastIndex + 1, (lastIndex + 3).coerceAtMost(hunkRows.size)).map(DiffRowUi::sourceText),
            body,
        )
        viewModelScope.launch {
            runCatching { repository.createDraft(input) }
                .onSuccess { draft ->
                    val drafts = (mutableState.value.drafts.filterNot { it.id == draft.id } + draft)
                    mutableState.value = mutableState.value.copy(drafts = drafts, editor = null, error = null)
                    persistDrafts()
                }
                .onFailure { mutableState.value = mutableState.value.copy(error = "Comment cannot be saved on that range") }
        }
    }

    fun deleteDraft(id: String) {
        val remaining = mutableState.value.drafts.filterNot { it.id == id }
        mutableState.value = mutableState.value.copy(
            drafts = remaining,
            needsAttention = mutableState.value.needsAttention && remaining.isNotEmpty(),
        )
        if (remaining.isEmpty() && mutableState.value.overallBody.isBlank()) {
            draftStore.clear(repositoryName, number)
        } else {
            persistDrafts()
        }
    }

    fun setOverallBody(body: String) {
        mutableState.value = mutableState.value.copy(overallBody = body)
        persistDrafts()
    }

    fun setVerdict(verdict: ReviewVerdictUi) { mutableState.value = mutableState.value.copy(verdict = verdict) }
    fun setConfirmation(open: Boolean) {
        mutableState.value = mutableState.value.copy(
            confirmation = open,
            finishing = if (open) false else mutableState.value.finishing,
        )
    }
    fun refreshAfterAttention() {
        if (mutableState.value.drafts.isEmpty()) {
            draftStore.clear(repositoryName, number)
            open()
        }
    }
    fun dismissSuccess() { mutableState.value = mutableState.value.copy(success = null) }

    fun publish() {
        val state = mutableState.value
        val pull = state.pullRequest ?: return
        if (state.publishing || state.needsAttention) return
        if (pull.author.equals(pull.viewer, ignoreCase = true) && state.verdict != ReviewVerdictUi.Comment) {
            mutableState.value = state.copy(error = "You can only leave a comment on your own pull request")
            return
        }
        if (state.verdict == ReviewVerdictUi.RequestChanges && state.overallBody.isBlank()) {
            mutableState.value = state.copy(error = "Request changes needs an overall comment")
            return
        }
        val review = state.toDraftReview() ?: return
        mutableState.value = state.copy(publishing = true)
        viewModelScope.launch {
            runCatching { repository.publish(review, state.verdict) }
                .onSuccess {
                    draftStore.clear(repositoryName, number)
                    mutableState.value = mutableState.value.copy(
                        publishing = false, confirmation = false, finishing = false, drafts = emptyList(),
                        overallBody = "", success = "Review published", error = null,
                    )
                }
                .onFailure { error ->
                    mutableState.value = mutableState.value.copy(
                        publishing = false,
                        confirmation = false,
                        needsAttention = error is MobileException.StaleRevision,
                        error = if (error is MobileException.StaleRevision) {
                            "The pull request changed. Your drafts are safe; refresh before publishing."
                        } else "Review was not published. Your drafts are safe."
                    )
                }
        }
    }

    fun setHorizontalOffset(offset: Int) {
        val state = mutableState.value
        mutableState.value = state.copy(horizontalOffsets = state.horizontalOffsets + (state.selectedFile to offset))
    }

    fun loadMoreRows() {
        val screen = mutableState.value.screen ?: return
        val next = screen.nextRow ?: return
        if (pageJob?.isActive == true) return
        pageJob = viewModelScope.launch {
            runCatching { repository.file(repositoryName, number, mutableState.value.selectedFile, next) }
                .onSuccess { page ->
                    val current = mutableState.value.screen ?: return@onSuccess
                    mutableState.value = mutableState.value.copy(
                        screen = current.copy(
                            rows = (current.rows + page.rows).distinctBy(DiffRowUi::key),
                            nextRow = page.nextRow,
                            threads = page.threads,
                        ),
                    )
                }
                .onFailure { mutableState.value = mutableState.value.copy(error = message(it)) }
        }
    }

    fun lastRowVisible() {
        val screen = mutableState.value.screen ?: return
        if (screen.nextRow == null && !screen.file.viewed) setViewed(true)
    }

    fun setViewed(viewed: Boolean) {
        val before = mutableState.value
        val screen = before.screen ?: return
        if (screen.file.viewed == viewed) return
        applyViewed(viewed)
        viewModelScope.launch {
            runCatching { repository.setViewed(screen.pullRequestId, screen.file.path, viewed) }
                .onFailure {
                    mutableState.value = before.copy(error = "Could not sync viewed state")
                }
        }
    }

    fun expand(row: DiffRowUi) {
        if (!row.key.contains(":gap:")) return
        viewModelScope.launch {
            runCatching { repository.expand(repositoryName, number, mutableState.value.selectedFile, row.key) }
                .onSuccess { mutableState.value = mutableState.value.copy(screen = it, error = null) }
                .onFailure { mutableState.value = mutableState.value.copy(error = "Could not expand this context") }
        }
    }

    private fun loadFile(index: Int) {
        pageJob?.cancel()
        mutableState.value = mutableState.value.copy(loading = true, screen = null, error = null)
        pageJob = viewModelScope.launch {
            runCatching { repository.file(repositoryName, number, index, 0) }
                .onSuccess { mutableState.value = mutableState.value.copy(loading = false, screen = it) }
                .onFailure { mutableState.value = mutableState.value.copy(loading = false, error = message(it)) }
        }
    }

    private fun applyViewed(viewed: Boolean) {
        val state = mutableState.value
        val screen = state.screen ?: return
        val delta = if (viewed) 1 else -1
        val pull = state.pullRequest?.let { pull ->
            pull.copy(files = pull.files.mapIndexed { index, file ->
                if (index == state.selectedFile) file.copy(viewed = viewed) else file
            })
        }
        mutableState.value = state.copy(
            pullRequest = pull,
            screen = screen.copy(
                file = screen.file.copy(viewed = viewed),
                viewedCount = (screen.viewedCount + delta).coerceIn(0, screen.fileCount),
            ),
        )
    }

    private fun message(error: Throwable) = error.message?.takeIf(String::isNotBlank) ?: "Could not load this pull request"

    private fun persistDrafts() {
        mutableState.value.toDraftReview()?.let(draftStore::save)
    }
}

private fun ReviewUiState.toDraftReview(): DraftReviewUi? = pullRequest?.let {
    DraftReviewUi(it.repository, it.number, it.revision, overallBody, drafts)
}

private fun DiffRowUi.side() = if (kind == LineKindUi.Deletion) CommentSideUi.Left else CommentSideUi.Right
private fun DiffRowUi.lineFor(side: CommentSideUi) = if (side == CommentSideUi.Left) oldLine else newLine
private fun DiffRowUi.sourceText() = spans.joinToString("") { it.text }
