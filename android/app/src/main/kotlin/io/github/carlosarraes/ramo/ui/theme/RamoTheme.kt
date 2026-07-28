package io.github.carlosarraes.ramo.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

val Background = Color(0xFF1A1B26)
val Surface = Color(0xFF24283B)
val TextPrimary = Color(0xFFC0CAF5)
val TextMuted = Color(0xFF565F89)
val Blue = Color(0xFF7AA2F7)
val Cyan = Color(0xFF7DCFFF)
val Green = Color(0xFF9ECE6A)
val Red = Color(0xFFF7768E)
val Amber = Color(0xFFE0AF68)
val Purple = Color(0xFFBB9AF7)

private val RamoColors = darkColorScheme(
    primary = Blue,
    onPrimary = Background,
    secondary = Cyan,
    tertiary = Purple,
    background = Background,
    onBackground = TextPrimary,
    surface = Surface,
    onSurface = TextPrimary,
    error = Red,
)

@Composable
fun RamoTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = RamoColors, content = content)
}
