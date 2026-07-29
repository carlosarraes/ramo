package io.github.carlosarraes.ramo.inbox

import kotlin.test.Test
import kotlin.test.assertEquals

class InboxFormattingTest {
    @Test
    fun relativeTimesStayCompactAndBounded() {
        assertEquals("18m", relativeAge(now = 2_000_000L, updated = 920_000L))
        assertEquals("3h", relativeAge(now = 12_000_000L, updated = 1_200_000L))
        assertEquals("2d", relativeAge(now = 172_800_000L, updated = 0L))
    }
}
