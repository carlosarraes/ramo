package io.github.carlosarraes.ramo.security

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class TokenStoreTest {
    @Test fun ciphertextHidesPlaintextAndCanBeCleared() {
        val context = ApplicationProvider.getApplicationContext<android.content.Context>()
        val store = EncryptedBlobStore(context, "ramo.mobile.test.aes", "token-test.enc")
        val secret = "github_pat_not_plaintext".toByteArray()
        store.clear()
        store.write(secret)
        assertFalse(store.storedBytesForTest()!!.toString(Charsets.UTF_8).contains("github_pat"))
        assertArrayEquals(secret, store.read())
        store.clear()
        assertNull(store.read())
    }
}
