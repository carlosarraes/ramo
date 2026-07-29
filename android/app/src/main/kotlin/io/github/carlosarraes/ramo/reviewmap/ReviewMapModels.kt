package io.github.carlosarraes.ramo.reviewmap

import java.net.URI
import java.net.URLDecoder
import java.nio.charset.StandardCharsets

data class PairingLink private constructor(val endpoint: String, val code: String) {
    override fun toString(): String = "PairingLink(endpoint=$endpoint, code=<redacted>)"

    companion object {
        fun parse(source: String?): PairingLink? = runCatching {
            val outer = URI(source ?: return null)
            if (outer.scheme != "ramo" || outer.host != "pair" || outer.fragment != null || outer.userInfo != null) {
                return null
            }
            val values = outer.rawQuery.orEmpty().split('&').mapNotNull { field ->
                val parts = field.split('=', limit = 2)
                if (parts.size != 2) null else decode(parts[0]) to decode(parts[1])
            }.toMap()
            val endpoint = URI(values["endpoint"] ?: return null)
            val code = values["code"] ?: return null
            if (endpoint.scheme != "https" || endpoint.host?.endsWith(".ts.net") != true ||
                endpoint.userInfo != null || endpoint.fragment != null || endpoint.query != null ||
                code.isBlank() || code.length > 256
            ) return null
            PairingLink(endpoint.toString().trimEnd('/'), code)
        }.getOrNull()

        private fun decode(value: String): String =
            URLDecoder.decode(value, StandardCharsets.UTF_8.name())
    }
}

enum class ReviewMapFailureCode {
    ServerUnreachable,
    PairingRejected,
    ClientUnauthorized,
    GithubAuthUnavailable,
    GithubRequestFailed,
    PullRequestUnavailable,
    OllamaUnavailable,
    ModelMissing,
    AnalysisTimedOut,
    AnalysisInvalid,
    AnalysisFailed,
    ResultStale,
    CacheUnavailable,
    ServerIncompatible,
}

enum class ReviewMapPhase { Exact, Analyzing, Enriched, Offline, Unpaired, Failed }

data class ReviewMapFailure(
    val code: ReviewMapFailureCode,
    val message: String,
    val retryable: Boolean = true,
)

data class ReviewMapResolveRequest(
    val repository: String,
    val pullRequest: Long,
    val expectedHeadSha: String,
)

enum class ReviewFileKindUi { Authored, Test, Generated, Migration, Documentation, Other }

data class ReviewMapFileUi(
    val id: String,
    val path: String,
    val additions: Long,
    val deletions: Long,
    val kind: ReviewFileKindUi,
    val owner: String? = null,
    val summary: String? = null,
    val risk: String? = null,
    val recommendedOrder: Int? = null,
    val viewed: Boolean = false,
)

data class ReviewMapGroupUi(
    val id: String,
    val label: String,
    val kind: ReviewFileKindUi,
    val fileIds: List<String>,
    val additions: Long,
    val deletions: Long,
    val collapsedByDefault: Boolean,
    val summary: String? = null,
    val risk: String? = null,
    val reviewPriority: Int? = null,
)

data class ReviewMapUi(
    val repository: String,
    val number: Long,
    val baseSha: String,
    val headSha: String,
    val additions: Long,
    val deletions: Long,
    val groups: List<ReviewMapGroupUi>,
    val files: List<ReviewMapFileUi>,
    val analysisModel: String? = null,
) {
    val fileById: Map<String, ReviewMapFileUi> = files.associateBy(ReviewMapFileUi::id)
}

data class ReviewMapServerResult(
    val jobId: String,
    val phase: ReviewMapPhase,
    val map: ReviewMapUi,
    val failure: ReviewMapFailure? = null,
)
