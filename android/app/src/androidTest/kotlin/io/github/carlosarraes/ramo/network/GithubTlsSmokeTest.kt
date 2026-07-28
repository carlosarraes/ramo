package io.github.carlosarraes.ramo.network

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.carlosarraes.ramo.uniffi.MobileException
import io.github.carlosarraes.ramo.uniffi.MobileSession
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class GithubTlsSmokeTest {
    @Test
    fun githubHandshakeReachesTypedAuthenticationFailure() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        check(NativeNetworkBootstrap.initialize(context) == BootstrapStatus.Ready)
        val session = MobileSession("deliberately_invalid_ramo_test_token")
        try {
            try {
                session.viewer()
                fail("GitHub unexpectedly accepted the deliberately invalid token")
            } catch (_: MobileException.InvalidCredentials) {
                // Reaching a typed 401 proves Android TLS verification completed.
            }
        } finally {
            session.close()
        }
    }
}
