package io.github.carlosarraes.ramo.reviewmap

import io.github.carlosarraes.ramo.security.ServerPairing
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.net.HttpURLConnection
import java.net.URL
import javax.net.ssl.HttpsURLConnection

class ReviewMapServerException(
    val code: ReviewMapFailureCode,
    message: String,
) : Exception(message) {
    override fun toString(): String = "ReviewMapServerException(code=$code, message=${message.orEmpty()})"
}

internal data class HttpResult(val status: Int, val body: String)

internal fun interface ReviewMapHttpEngine {
    fun request(endpoint: String, path: String, method: String, token: String?, body: String?): HttpResult
}

internal object HttpsReviewMapEngine : ReviewMapHttpEngine {
    override fun request(endpoint: String, path: String, method: String, token: String?, body: String?): HttpResult {
        val connection = URL(endpoint + path).openConnection() as? HttpsURLConnection
            ?: throw ReviewMapServerException(ReviewMapFailureCode.ServerIncompatible, "Local analysis requires HTTPS")
        try {
            connection.instanceFollowRedirects = false
            connection.connectTimeout = 10_000
            connection.readTimeout = 10_000
            connection.requestMethod = method
            connection.setRequestProperty("Accept", "application/json")
            connection.setRequestProperty("Content-Type", "application/json")
            token?.let { connection.setRequestProperty("Authorization", "Bearer $it") }
            if (body != null) {
                connection.doOutput = true
                connection.outputStream.use { it.write(body.toByteArray()) }
            }
            val status = connection.responseCode
            if (status in 300..399) {
                throw ReviewMapServerException(ReviewMapFailureCode.ServerIncompatible, "Local analysis refused a redirect")
            }
            val stream = if (status >= 400) connection.errorStream else connection.inputStream
            val bytes = stream?.use { input ->
                val output = ByteArrayOutputStream()
                val buffer = ByteArray(8 * 1024)
                while (true) {
                    val count = input.read(buffer)
                    if (count < 0) break
                    if (output.size() + count > MAX_RESPONSE_BYTES) {
                        throw ReviewMapServerException(ReviewMapFailureCode.ServerIncompatible, "Local analysis response is too large")
                    }
                    output.write(buffer, 0, count)
                }
                output.toByteArray()
            } ?: ByteArray(0)
            return HttpResult(status, bytes.toString(Charsets.UTF_8))
        } finally {
            connection.disconnect()
        }
    }

    private const val MAX_RESPONSE_BYTES = 2 * 1024 * 1024
}

class ReviewMapServerClient internal constructor(
    private val pairing: ServerPairing,
    private val engine: ReviewMapHttpEngine = HttpsReviewMapEngine,
) {
    suspend fun resolve(request: ReviewMapResolveRequest): ReviewMapServerResult = call(
        "/v1/review-maps",
        "POST",
        JSONObject()
            .put("schema_version", 1)
            .put("repository", request.repository)
            .put("pull_request", request.pullRequest)
            .put("expected_head_sha", request.expectedHeadSha)
            .toString(),
    )

    suspend fun poll(jobId: String): ReviewMapServerResult =
        call("/v1/review-maps/${segment(jobId)}", "GET", null)

    suspend fun retry(jobId: String): ReviewMapServerResult =
        call("/v1/review-maps/${segment(jobId)}/retry", "POST", "{}")

    suspend fun revoke() = withContext(Dispatchers.IO) {
        val response = engine.request(
            pairing.endpoint,
            "/v1/clients/${segment(pairing.clientId)}",
            "DELETE",
            pairing.token,
            null,
        )
        if (response.status != HttpURLConnection.HTTP_NO_CONTENT) throwFailure(response)
    }

    private suspend fun call(path: String, method: String, body: String?): ReviewMapServerResult =
        withContext(Dispatchers.IO) {
            val response = try {
                engine.request(pairing.endpoint, path, method, pairing.token, body)
            } catch (error: ReviewMapServerException) {
                throw error
            } catch (_: Exception) {
                throw ReviewMapServerException(ReviewMapFailureCode.ServerUnreachable, "Could not reach laptop analysis")
            }
            if (response.status !in 200..299) throwFailure(response)
            parseResult(response.body)
        }

    companion object {
        suspend fun exchange(link: PairingLink): ServerPairing = exchange(link, HttpsReviewMapEngine)

        internal suspend fun exchange(
            link: PairingLink,
            engine: ReviewMapHttpEngine,
        ): ServerPairing = withContext(Dispatchers.IO) {
            val body = JSONObject().put("code", link.code).put("label", "Ramo Android").toString()
            val response = try {
                engine.request(link.endpoint, "/v1/pair/exchange", "POST", null, body)
            } catch (error: ReviewMapServerException) {
                throw error
            } catch (_: Exception) {
                throw ReviewMapServerException(ReviewMapFailureCode.ServerUnreachable, "Could not reach laptop analysis")
            }
            if (response.status !in 200..299) throwFailure(response)
            val json = JSONObject(response.body)
            val clientId = json.optString("client_id")
            val token = json.optString("token")
            if (clientId.isBlank() || token.isBlank()) {
                throw ReviewMapServerException(ReviewMapFailureCode.ServerIncompatible, "Pairing response is incomplete")
            }
            ServerPairing(link.endpoint, clientId, token, System.currentTimeMillis())
        }

        private fun throwFailure(response: HttpResult): Nothing {
            val failure = runCatching { JSONObject(response.body).getJSONObject("failure") }.getOrNull()
            val code = failure?.optString("code")?.let(::failureCode)
                ?: if (response.status == HttpURLConnection.HTTP_UNAUTHORIZED) {
                    ReviewMapFailureCode.ClientUnauthorized
                } else ReviewMapFailureCode.ServerIncompatible
            throw ReviewMapServerException(code, failureMessage(code))
        }

        private fun parseResult(source: String): ReviewMapServerResult {
            val root = JSONObject(source)
            if (root.optInt("schema_version") != 1) {
                throw ReviewMapServerException(ReviewMapFailureCode.ServerIncompatible, "Review Map schema is incompatible")
            }
            val map = parseMap(root.getJSONObject("map"))
            val failure = root.optJSONObject("failure")?.let {
                val code = failureCode(it.optString("code"))
                ReviewMapFailure(code, failureMessage(code))
            }
            return ReviewMapServerResult(
                root.getString("job_id"),
                phase(root.optString("state")),
                map,
                failure,
            )
        }

        private fun parseMap(root: JSONObject): ReviewMapUi {
            val identity = root.getJSONObject("identity")
            val totals = root.getJSONObject("totals")
            val files = root.getJSONArray("files").let { array ->
                List(array.length()) { index ->
                    val file = array.getJSONObject(index)
                    val insight = file.optJSONObject("insight")
                    ReviewMapFileUi(
                        file.getString("id"), file.getString("path"),
                        file.getLong("additions"), file.getLong("deletions"),
                        kind(file.getString("kind")), file.optNullableString("owner"),
                        insight?.optNullableString("summary"), insight?.optNullableString("risk"),
                        file.optNullableInt("recommended_order"),
                    )
                }
            }
            val groups = root.getJSONArray("groups").let { array ->
                List(array.length()) { index ->
                    val group = array.getJSONObject(index)
                    val ids = group.getJSONArray("file_ids")
                    val insight = group.optJSONObject("insight")
                    ReviewMapGroupUi(
                        group.getString("id"), group.getString("label"), kind(group.getString("kind")),
                        List(ids.length()) { ids.getString(it) }, group.getLong("additions"),
                        group.getLong("deletions"), group.getBoolean("collapsed_by_default"),
                        insight?.optNullableString("summary"), insight?.optNullableString("risk"),
                        insight?.optNullableInt("review_priority"),
                    )
                }
            }
            return ReviewMapUi(
                identity.getString("repository"), identity.getLong("pull_request"),
                identity.getString("base_sha"), identity.getString("head_sha"),
                totals.getLong("additions"), totals.getLong("deletions"), groups, files,
                root.optJSONObject("analysis")?.optNullableString("model"),
            )
        }

        private fun phase(value: String) = when (value) {
            "analyzing", "ready" -> ReviewMapPhase.Analyzing
            "enriched" -> ReviewMapPhase.Enriched
            "stale" -> ReviewMapPhase.Offline
            "unavailable" -> ReviewMapPhase.Offline
            "failed" -> ReviewMapPhase.Failed
            else -> ReviewMapPhase.Failed
        }

        private fun kind(value: String) = when (value) {
            "authored" -> ReviewFileKindUi.Authored
            "test" -> ReviewFileKindUi.Test
            "generated" -> ReviewFileKindUi.Generated
            "migration" -> ReviewFileKindUi.Migration
            "documentation" -> ReviewFileKindUi.Documentation
            else -> ReviewFileKindUi.Other
        }

        private fun failureCode(value: String) = ReviewMapFailureCode.entries.firstOrNull {
            it.name.replace(Regex("([a-z])([A-Z])"), "$1_$2").lowercase() == value
        } ?: ReviewMapFailureCode.ServerIncompatible

        private fun failureMessage(code: ReviewMapFailureCode) = when (code) {
            ReviewMapFailureCode.ServerUnreachable -> "Could not reach laptop analysis"
            ReviewMapFailureCode.PairingRejected -> "The pairing code expired or was already used"
            ReviewMapFailureCode.ClientUnauthorized -> "This phone is no longer paired"
            ReviewMapFailureCode.OllamaUnavailable -> "Ollama is not available on the laptop"
            ReviewMapFailureCode.ModelMissing -> "The selected local model is not installed"
            ReviewMapFailureCode.AnalysisTimedOut -> "Local analysis timed out"
            ReviewMapFailureCode.ResultStale -> "The pull request changed; refresh the map"
            ReviewMapFailureCode.ServerIncompatible -> "Ramo and the laptop server are incompatible"
            else -> "Laptop analysis is unavailable; the exact map is still ready"
        }

        private fun segment(value: String): String {
            require(value.isNotBlank() && value.all { it.isLetterOrDigit() || it in "-_" })
            return value
        }
    }
}

private fun JSONObject.optNullableString(key: String): String? =
    takeUnless { isNull(key) }?.optString(key)?.takeIf(String::isNotBlank)

private fun JSONObject.optNullableInt(key: String): Int? =
    takeUnless { isNull(key) }?.optInt(key)?.takeIf { has(key) }
