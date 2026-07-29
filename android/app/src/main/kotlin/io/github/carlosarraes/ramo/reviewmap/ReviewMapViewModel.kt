package io.github.carlosarraes.ramo.reviewmap

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

data class ReviewMapUiState(
    val loading: Boolean = true,
    val map: ReviewMapUi? = null,
    val phase: ReviewMapPhase = ReviewMapPhase.Exact,
    val expandedGroups: Set<String> = emptySet(),
    val selectedId: String? = null,
    val reviewedPaths: Set<String> = emptySet(),
    val failure: ReviewMapFailure? = null,
    val aiHidden: Boolean = false,
)

class ReviewMapViewModel(
    private val repository: ReviewMapRepository,
    private val repositoryName: String,
    private val number: Long,
) : ViewModel() {
    private val mutableState = MutableStateFlow(ReviewMapUiState())
    val state: StateFlow<ReviewMapUiState> = mutableState.asStateFlow()
    private var job: Job? = null

    init { open() }

    fun open() {
        job?.cancel()
        job = viewModelScope.launch {
            try {
                val exact = repository.exact(repositoryName, number)
                val initialExpanded = exact.groups.filterNot(ReviewMapGroupUi::collapsedByDefault).mapTo(mutableSetOf()) { it.id }
                mutableState.value = mutableState.value.copy(
                    loading = false, map = exact,
                    phase = if (repository.isPaired()) ReviewMapPhase.Analyzing else ReviewMapPhase.Unpaired,
                    expandedGroups = mutableState.value.expandedGroups.ifEmpty { initialExpanded },
                    selectedId = mutableState.value.selectedId ?: exact.files.firstOrNull()?.id,
                    reviewedPaths = exact.files.filter(ReviewMapFileUi::viewed).mapTo(mutableSetOf()) { it.path },
                    failure = null,
                )
                if (!repository.isPaired() || mutableState.value.aiHidden) return@launch
                val first = repository.resolve(ReviewMapResolveRequest(repositoryName, number, exact.headSha)) ?: return@launch
                apply(first, exact.headSha)
                poll(first)
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: ReviewMapServerException) {
                mutableState.value = mutableState.value.copy(
                    phase = ReviewMapPhase.Offline,
                    failure = ReviewMapFailure(error.code, error.message ?: "Laptop analysis unavailable"),
                )
            } catch (_: Exception) {
                mutableState.value = mutableState.value.copy(
                    loading = false, phase = ReviewMapPhase.Failed,
                    failure = ReviewMapFailure(ReviewMapFailureCode.ServerUnreachable, "Could not open this Review Map"),
                )
            }
        }
    }

    fun toggleGroup(id: String) {
        val expanded = mutableState.value.expandedGroups.toMutableSet()
        if (!expanded.add(id)) expanded.remove(id)
        mutableState.value = mutableState.value.copy(expandedGroups = expanded, selectedId = id)
    }

    fun select(id: String) { mutableState.value = mutableState.value.copy(selectedId = id) }
    fun dismissFailure() { mutableState.value = mutableState.value.copy(failure = null) }
    fun hideAi() { job?.cancel(); mutableState.value = mutableState.value.copy(aiHidden = true, phase = ReviewMapPhase.Exact, failure = null) }
    fun retry() { open() }
    fun markReviewed(path: String) {
        mutableState.value = mutableState.value.copy(reviewedPaths = mutableState.value.reviewedPaths + path)
    }

    private suspend fun poll(initial: ReviewMapServerResult) {
        var result = initial
        val delays = longArrayOf(250, 500, 1_000, 2_000)
        var attempt = 0
        while (result.phase == ReviewMapPhase.Analyzing) {
            delay(delays[attempt.coerceAtMost(delays.lastIndex)])
            attempt++
            result = repository.poll(result.jobId)
            apply(result, mutableState.value.map?.headSha ?: return)
        }
    }

    private fun apply(result: ReviewMapServerResult, expectedHead: String) {
        if (result.map.headSha != expectedHead) {
            mutableState.value = mutableState.value.copy(
                phase = ReviewMapPhase.Offline,
                failure = ReviewMapFailure(ReviewMapFailureCode.ResultStale, "The pull request changed; refresh the map"),
            )
            return
        }
        mutableState.value = mutableState.value.copy(
            map = if (mutableState.value.aiHidden) mutableState.value.map else result.map,
            phase = if (mutableState.value.aiHidden) ReviewMapPhase.Exact else result.phase,
            failure = result.failure,
        )
    }
}
