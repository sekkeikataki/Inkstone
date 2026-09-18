package dev.inkstone.android.ui

import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

private val Paper = Color(0xFFFAFAF7)
private val Ink = Color(0xFF1A1F29)
private val Accent = Color(0xFF2E6B49)
private val AccentSoft = Color(0xFFD7E8DC)

private val LightColors: ColorScheme = lightColorScheme(
    primary = Accent,
    onPrimary = Color.White,
    secondary = Accent,
    background = Paper,
    surface = Color.White,
    onBackground = Ink,
    onSurface = Ink,
    surfaceVariant = AccentSoft,
)

private val DarkColors: ColorScheme = darkColorScheme(
    primary = Color(0xFF8FCBAA),
    onPrimary = Color(0xFF0E1A14),
    background = Color(0xFF121512),
    surface = Color(0xFF1A1F1C),
    onBackground = Color(0xFFE8EDE8),
    onSurface = Color(0xFFE8EDE8),
)

@Composable
fun InkstoneTheme(dark: Boolean = false, content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = if (dark) DarkColors else LightColors,
        content = content,
    )
}
