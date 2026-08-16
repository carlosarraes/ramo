package io.github.carlosarraes.ramo.data

import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals

class RamoBridgeTest {
    @Test
    fun reportsCoreVersion() = runTest {
        val bridge = NativeRamoBridge()
        assertEquals("0.0.20", bridge.coreVersion())
    }
}
