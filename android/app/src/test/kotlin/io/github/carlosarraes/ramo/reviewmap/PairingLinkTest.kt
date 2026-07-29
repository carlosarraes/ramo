package io.github.carlosarraes.ramo.reviewmap

import kotlin.test.Test
import kotlin.test.assertNotNull
import kotlin.test.assertNull

class PairingLinkTest {
    @Test
    fun pairingLinkRejectsHttpAndNonTailscaleHosts() {
        assertNull(PairingLink.parse("ramo://pair?endpoint=http%3A%2F%2Flaptop.ts.net&code=x"))
        assertNull(PairingLink.parse("ramo://pair?endpoint=https%3A%2F%2Fexample.com&code=x"))
        assertNotNull(PairingLink.parse("ramo://pair?endpoint=https%3A%2F%2Flaptop.tail123.ts.net&code=x"))
    }

    @Test
    fun pairingSecretsAreNeverRendered() {
        val link = PairingLink.parse(
            "ramo://pair?endpoint=https%3A%2F%2Flaptop.tail123.ts.net&code=one-time-secret",
        )!!
        assertNull(link.toString().takeIf { it.contains("one-time-secret") })
    }
}
