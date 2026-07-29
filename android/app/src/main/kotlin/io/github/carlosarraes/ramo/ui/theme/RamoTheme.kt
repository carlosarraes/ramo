package io.github.carlosarraes.ramo.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface as MaterialSurface
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag

val Background = Color(0xFF111821)
val Surface = Color(0xFF17212B)
val SurfaceVariant = Color(0xFF202C38)
val TextPrimary = Color(0xFFF2F5F8)
val TextSecondary = Color(0xFFB7C2CE)
val TextMuted = Color(0xFF8795A5)
val Blue = Color(0xFF4F8CFF)
val Cyan = Color(0xFF7DCFFF)
val Green = Color(0xFF92D27D)
val Red = Color(0xFFFF7D8D)
val Amber = Color(0xFFE0AF68)
val Purple = Color(0xFFBB9AF7)
private val Outline = Color(0xFF536476)
private val ErrorContainer = Color(0xFF4A202A)

private val RamoColors = darkColorScheme(
    primary = Blue,
    onPrimary = Background,
    primaryContainer = SurfaceVariant,
    onPrimaryContainer = TextPrimary,
    secondary = Cyan,
    onSecondary = Background,
    secondaryContainer = SurfaceVariant,
    onSecondaryContainer = TextPrimary,
    tertiary = Purple,
    onTertiary = Background,
    tertiaryContainer = SurfaceVariant,
    onTertiaryContainer = TextPrimary,
    background = Background,
    onBackground = TextPrimary,
    surface = Surface,
    onSurface = TextPrimary,
    surfaceVariant = SurfaceVariant,
    onSurfaceVariant = TextSecondary,
    outline = Outline,
    outlineVariant = Outline.copy(alpha = 0.55f),
    error = Red,
    onError = Background,
    errorContainer = ErrorContainer,
    onErrorContainer = TextPrimary,
    inverseSurface = TextPrimary,
    inverseOnSurface = Background,
    inversePrimary = Blue,
)

@Composable
fun RamoTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = RamoColors, content = content)
}

@Composable
fun RamoAppSurface(content: @Composable () -> Unit) {
    RamoTheme {
        MaterialSurface(
            modifier = Modifier.fillMaxSize().testTag("ramo-root"),
            color = MaterialTheme.colorScheme.background,
            contentColor = MaterialTheme.colorScheme.onBackground,
            content = content,
        )
    }
}
