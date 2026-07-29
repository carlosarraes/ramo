package io.github.carlosarraes.ramo.ui

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import io.github.carlosarraes.ramo.ui.theme.Background
import io.github.carlosarraes.ramo.ui.theme.RamoAppSurface
import io.github.carlosarraes.ramo.ui.theme.TextPrimary
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class RamoThemeTest {
    @get:Rule val compose = createComposeRule()

    @Test
    fun appSurfaceProvidesTheApprovedReadablePalette() {
        var background = Color.Unspecified
        var content = Color.Unspecified

        compose.setContent {
            RamoAppSurface {
                background = MaterialTheme.colorScheme.background
                content = MaterialTheme.colorScheme.onBackground
            }
        }

        compose.onNodeWithTag("ramo-root").assertExists()
        compose.runOnIdle {
            assertEquals(Background, background)
            assertEquals(TextPrimary, content)
        }

    }

    @Test
    @GraphicsMode(GraphicsMode.Mode.NATIVE)
    fun paletteRendersAsDistinctNonBlackPixels() {
        val bitmap = Bitmap.createBitmap(20, 10, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        canvas.drawColor(Background.toArgb())
        canvas.drawRect(10f, 0f, 20f, 10f, Paint().apply { color = TextPrimary.toArgb() })

        assertEquals(Background.toArgb(), bitmap.getPixel(2, 5))
        assertEquals(TextPrimary.toArgb(), bitmap.getPixel(15, 5))
        assertNotEquals(Color.Black.toArgb(), bitmap.getPixel(2, 5))
        assertNotEquals(bitmap.getPixel(2, 5), bitmap.getPixel(15, 5))
    }
}
