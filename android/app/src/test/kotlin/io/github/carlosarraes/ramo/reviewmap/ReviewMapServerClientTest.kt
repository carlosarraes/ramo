package io.github.carlosarraes.ramo.reviewmap

import io.github.carlosarraes.ramo.security.ServerPairing
import kotlinx.coroutines.test.runTest
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertFailsWith

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ReviewMapServerClientTest {
    @Test
    fun incompatibleSchemaFallsBackWithoutLeakingToken() = runTest {
        val engine = ReviewMapHttpEngine { _, _, _, _, _ -> HttpResult(200, """{"schema_version":99}""") }
        val client = ReviewMapServerClient(ServerPairing("https://laptop.tail.ts.net", "id", "secret", 0), engine)

        val error = assertFailsWith<ReviewMapServerException> {
            client.resolve(ReviewMapResolveRequest("owner/repo", 7, "head"))
        }

        assertEquals(ReviewMapFailureCode.ServerIncompatible, error.code)
        assertFalse(error.toString().contains("secret"))
    }

    @Test
    fun pairingPersistsOnlyACompleteCredential() = runTest {
        val link = PairingLink.parse(
            "ramo://pair?endpoint=https%3A%2F%2Flaptop.tail.ts.net&code=once",
        )!!
        val engine = ReviewMapHttpEngine { _, _, _, _, _ ->
            HttpResult(200, """{"client_id":"phone","token":"ramo_secret"}""")
        }

        val pairing = ReviewMapServerClient.exchange(link, engine)

        assertEquals("phone", pairing.clientId)
        assertFalse(pairing.toString().contains("ramo_secret"))
    }

    @Test
    fun serverFailureTextCannotReachTheUiOrException() = runTest {
        val engine = ReviewMapHttpEngine { _, _, _, _, _ ->
            HttpResult(401, """{"failure":{"code":"client_unauthorized","message":"secret reflected text"}}""")
        }
        val client = ReviewMapServerClient(ServerPairing("https://laptop.tail.ts.net", "id", "token", 0), engine)

        val error = assertFailsWith<ReviewMapServerException> {
            client.resolve(ReviewMapResolveRequest("owner/repo", 7, "head"))
        }

        assertEquals("This phone is no longer paired", error.message)
        assertFalse(error.toString().contains("reflected"))
    }

    @Test
    fun lowQualityResultUsesTypedSafeTextAndKeepsTheExactMap() = runTest {
        val engine = ReviewMapHttpEngine { _, _, _, _, _ ->
            HttpResult(200, lowQualityResponse("private reflected model output"))
        }
        val client = ReviewMapServerClient(
            ServerPairing("https://laptop.tail.ts.net", "id", "token", 0),
            engine,
        )

        val result = client.resolve(ReviewMapResolveRequest("owner/repo", 7, "head"))

        assertEquals(ReviewMapPhase.Failed, result.phase)
        assertEquals(ReviewMapFailureCode.AnalysisLowQuality, result.failure?.code)
        assertEquals(
            "AI analysis was not useful enough; the exact map is still ready",
            result.failure?.message,
        )
        assertEquals("src/lib.rs", result.map.files.single().path)
        assertFalse(result.failure?.message.orEmpty().contains("reflected"))
    }

    @Test
    fun pairingShowsTheTypedSafeFailureInsteadOfAGenericError() {
        val error = ReviewMapServerException(
            ReviewMapFailureCode.ServerUnreachable,
            "Could not reach laptop analysis",
        )

        assertEquals(
            "Could not reach laptop analysis. Turn on Tailscale on this phone, then try again.",
            pairingFailureMessage(error),
        )
        assertEquals("Could not pair laptop analysis", pairingFailureMessage(IllegalStateException("private")))
    }


    private fun lowQualityResponse(serverMessage: String) = """
        {
          "schema_version": 1,
          "job_id": "job-1",
          "state": "failed",
          "failure": {"code": "analysis_low_quality", "message": "$serverMessage"},
          "map": {
            "identity": {
              "repository": "owner/repo",
              "pull_request": 7,
              "base_sha": "base",
              "head_sha": "head"
            },
            "totals": {"additions": 3, "deletions": 1},
            "groups": [{
              "id": "group", "label": "src/", "kind": "authored",
              "file_ids": ["file"], "additions": 3, "deletions": 1,
              "collapsed_by_default": false
            }],
            "files": [{
              "id": "file", "path": "src/lib.rs", "kind": "authored",
              "additions": 3, "deletions": 1
            }]
          }
        }
    """.trimIndent()
}
