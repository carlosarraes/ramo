package io.github.carlosarraes.ramo.security

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ServerPairingStoreTest {
    @Test
    fun credentialIsEncryptedAtRestAndRoundTrips() {
        val store = ServerPairingStore(ApplicationProvider.getApplicationContext())
        store.clear()
        val pairing = ServerPairing(
            "https://laptop.tail123.ts.net",
            "phone",
            "ramo_private_token",
            42,
        )

        store.write(pairing)

        assertEquals(pairing, store.read())
        val stored = store.storedBytesForTest()!!.toString(Charsets.UTF_8)
        assertFalse(stored.contains("ramo_private_token"))
        assertFalse(stored.contains("laptop.tail123.ts.net"))
        store.clear()
    }
}
