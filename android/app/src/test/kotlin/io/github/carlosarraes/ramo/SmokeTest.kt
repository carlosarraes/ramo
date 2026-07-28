package io.github.carlosarraes.ramo

import kotlin.test.Test
import kotlin.test.assertEquals

class SmokeTest {
    @Test
    fun appNameIsRamo() = assertEquals("Ramo", BuildConfig.APP_NAME)
}
