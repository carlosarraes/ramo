package io.github.carlosarraes.ramo.review

import androidx.compose.ui.graphics.toArgb
import kotlin.test.Test
import kotlin.test.assertEquals

class SyntaxColorTest {
    @Test
    fun argbFromRustUsesThe32BitColorConstructor() {
        assertEquals(0xffc0caf5.toInt(), syntaxColor(0xffc0caf5).toArgb())
    }

    @Test
    fun highAndLowLongBitsCannotSelectAComposeColorSpace() {
        assertEquals(0xff112233.toInt(), syntaxColor(0x7fff_ffff_ff11_2233).toArgb())
    }
}
