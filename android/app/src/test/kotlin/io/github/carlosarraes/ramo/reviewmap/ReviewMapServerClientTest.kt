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
}
