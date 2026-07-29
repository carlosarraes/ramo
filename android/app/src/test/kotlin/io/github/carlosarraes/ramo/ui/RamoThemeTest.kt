package io.github.carlosarraes.ramo.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import io.github.carlosarraes.ramo.ui.theme.Background
import io.github.carlosarraes.ramo.ui.theme.RamoAppSurface
import io.github.carlosarraes.ramo.ui.theme.TextPrimary
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
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
}
