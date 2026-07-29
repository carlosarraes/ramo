package io.github.carlosarraes.ramo.review

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import io.github.carlosarraes.ramo.errors.toUserFacingFailure
import io.github.carlosarraes.ramo.uniffi.MobileException
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

class ReviewViewModel(
    private val repository: ReviewRepository,
    private val repositoryName: String,
    private val number: Long,
    private val draftStore: ReviewDraftStore = NoopDraftStore(),
) : ViewModel() {
    private val mutableState = MutableStateFlow(ReviewUiState())
    val state: StateFlow<ReviewUiState> = mutableState.asStateFlow()
    private var pageJob: Job? = null
    private val viewedJobs = mutableMapOf<Int, Job>()
    private val viewedMutations = mutableMapOf<Int, Long>()
    private var viewedMutationId = 0L
    private var noticeId = 0L

    init { open() }

    fun open() {
        viewedJobs.values.forEach(Job::cancel)
        viewedJobs.clear()
        viewedMutations.clear()
        mutableState.value = mutableState.value.copy(
            loading = true,
            screen = null,
            fileSheetOpen = false,
            error = null,
            selection = null,
            editor = null,
        )
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
        mutableState.value = mutableState.value.copy(
            selectedFile = index,
            fileSheetOpen = false,
            selection = null,
            editor = null,
        )
        loadFile(index)
    }

    fun previousFile() = selectFile(mutableState.value.selectedFile - 1)
    fun nextFile() = selectFile(mutableState.value.selectedFile + 1)
    fun setFileSheet(open: Boolean) { mutableState.value = mutableState.value.copy(fileSheetOpen = open) }
    fun setSummaryExpanded(expanded: Boolean) {
        mutableState.value = mutableState.value.copy(summaryExpanded = expanded)
    }
    fun setFinishing(open: Boolean) { mutableState.value = mutableState.value.copy(finishing = open) }

    fun selectLine(row: DiffRowUi) {
        if (!row.commentable) return
        val side = if (row.kind == LineKindUi.Deletion) CommentSideUi.Left else CommentSideUi.Right
        val line = if (side == CommentSideUi.Left) row.oldLine else row.newLine
        if (line == null) return

        val state = mutableState.value
        val current = state.selection
        val compatible = current?.takeIf { it.side == side && it.hunk == row.hunkIndex }
        val candidate = compatible?.let {
            LineSelectionUi(side, row.hunkIndex, minOf(it.startLine, line), maxOf(it.endLine, line))
        }
        val availableLines = state.screen?.rows.orEmpty()
            .asSequence()
            .filter { it.commentable && it.hunkIndex == row.hunkIndex && it.side() == side }
            .mapNotNull { it.lineFor(side) }
            .toSet()
        val contiguous = candidate?.let { selection ->
            (selection.startLine..selection.endLine).all(availableLines::contains)
        } == true
        mutableState.value = state.copy(
            selection = if (contiguous) candidate else LineSelectionUi(side, row.hunkIndex, line, line),
            editor = null,
        )
    }

    fun openComment() {
        mutableState.value.selection?.let { selection ->
            mutableState.value = mutableState.value.copy(editor = DraftEditorUi(selection))
        }
    }

    fun clearSelection() {
        mutableState.value = mutableState.value.copy(selection = null, editor = null)
    }

    fun cancelEditor() { mutableState.value = mutableState.value.copy(editor = null) }

    fun saveDraft(body: String) {
        val state = mutableState.value
        val editor = state.editor ?: return
        val screen = state.screen ?: return
        val pull = state.pullRequest ?: return
        val selection = editor.selection
        val hunkRows = screen.rows.filter { it.hunkIndex == selection.hunk && it.side() == selection.side }
        val selected = hunkRows.filter {
            (it.lineFor(selection.side) ?: -1) in selection.startLine..selection.endLine
        }
        if (selected.isEmpty()) return
        val firstIndex = hunkRows.indexOf(selected.first())
        val lastIndex = hunkRows.indexOf(selected.last())
        val input = DraftInputUi(
            pull.repository, pull.number, pull.revision, screen.file.path, selection.side,
            selection.startLine, selection.endLine, selection.hunk,
            hunkRows.subList((firstIndex - 2).coerceAtLeast(0), firstIndex).map(DiffRowUi::sourceText),
            selected.map(DiffRowUi::sourceText),
            hunkRows.subList(lastIndex + 1, (lastIndex + 3).coerceAtMost(hunkRows.size)).map(DiffRowUi::sourceText),
            body,
        )
        viewModelScope.launch {
            runCatching { repository.createDraft(input) }
                .onSuccess { draft ->
                    val drafts = (mutableState.value.drafts.filterNot { it.id == draft.id } + draft)
                    mutableState.value = mutableState.value.copy(
                        drafts = drafts,
                        selection = null,
                        editor = null,
                        error = null,
                    )
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
    fun dismissError() { mutableState.value = mutableState.value.copy(error = null) }

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
        if (screen.nextRow == null && !screen.file.viewed) {
            mutateViewed(mutableState.value.selectedFile, viewed = true, offerUndo = true)
        }
    }

    fun setViewed(viewed: Boolean) {
        mutateViewed(mutableState.value.selectedFile, viewed, offerUndo = false)
    }

    fun undoViewed() {
        val fileIndex = mutableState.value.notice?.undoViewedFile ?: return
        mutableState.value = mutableState.value.copy(notice = null)
        mutateViewed(fileIndex, viewed = false, offerUndo = false)
    }

    fun dismissNotice() { mutableState.value = mutableState.value.copy(notice = null) }

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

    private fun mutateViewed(index: Int, viewed: Boolean, offerUndo: Boolean) {
        val state = mutableState.value
        val pull = state.pullRequest ?: return
        val file = pull.files.getOrNull(index) ?: return
        if (file.viewed == viewed && viewedJobs[index]?.isActive != true) return
        val beforeViewed = file.viewed
        val pullRequestId = state.screen?.pullRequestId ?: pull.nodeId
        viewedJobs.remove(index)?.cancel()
        val mutationId = ++viewedMutationId
        viewedMutations[index] = mutationId
        applyViewed(index, viewed)
        if (offerUndo) {
            mutableState.value = mutableState.value.copy(
                notice = ReviewNoticeUi(++noticeId, "Marked viewed", index),
            )
        }
        val job = viewModelScope.launch {
            runCatching { repository.setViewed(pullRequestId, file.path, viewed) }
                .onFailure {
                    if (viewedMutations[index] == mutationId) {
                        applyViewed(index, beforeViewed)
                        mutableState.value = mutableState.value.copy(
                            notice = null,
                            error = "Could not sync viewed state",
                        )
                    }
                }
        }
        viewedJobs[index] = job
        job.invokeOnCompletion {
            if (viewedMutations[index] == mutationId) {
                viewedJobs.remove(index, job)
                viewedMutations.remove(index)
            }
        }
    }

    private fun applyViewed(index: Int, viewed: Boolean) {
        val state = mutableState.value
        val pullRequest = state.pullRequest ?: return
        val before = pullRequest.files.getOrNull(index) ?: return
        if (before.viewed == viewed) return
        val delta = if (viewed) 1 else -1
        val pull = pullRequest.copy(files = pullRequest.files.mapIndexed { fileIndex, file ->
            if (fileIndex == index) file.copy(viewed = viewed) else file
        })
        val screen = state.screen?.let { screen ->
            screen.copy(
                file = if (state.selectedFile == index) screen.file.copy(viewed = viewed) else screen.file,
                viewedCount = (screen.viewedCount + delta).coerceIn(0, screen.fileCount),
            )
        }
        mutableState.value = state.copy(
            pullRequest = pull,
            screen = screen,
        )
    }

    private fun message(error: Throwable) =
        error.toUserFacingFailure("Could not load this pull request").message

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
