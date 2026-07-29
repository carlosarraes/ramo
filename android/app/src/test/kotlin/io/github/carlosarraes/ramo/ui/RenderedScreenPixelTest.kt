package io.github.carlosarraes.ramo.ui

import android.graphics.Bitmap
import android.graphics.Canvas
import android.os.Looper
import android.view.View
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.ui.graphics.toArgb
import io.github.carlosarraes.ramo.inbox.InboxItem
import io.github.carlosarraes.ramo.inbox.InboxScreen
import io.github.carlosarraes.ramo.inbox.InboxUiState
import io.github.carlosarraes.ramo.inbox.TabState
import io.github.carlosarraes.ramo.ui.theme.Background
import io.github.carlosarraes.ramo.ui.theme.RamoAppSurface
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w360dp-h800dp")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class RenderedScreenPixelTest {
    @Test
    fun composedInboxRendersReadableForegroundAndBackgroundPixels() {
        val activity = Robolectric.buildActivity(ComponentActivity::class.java).setup().get()
        activity.setContent {
            RamoAppSurface {
                InboxScreen(
                    login = "reviewer",
                    state = renderedInboxState(),
                    nowMillis = 2_000_000L,
                    onSelect = {},
                    onQuery = {},
                    onDismissFailure = {},
                    onRefresh = {},
                    onLoadMore = {},
                    onOpen = {},
                    onSettings = {},
                    onSignOut = {},
                )
            }
        }
        shadowOf(Looper.getMainLooper()).idle()

        val root = activity.window.decorView
        val width = 360
        val height = 800
        root.measure(
            View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY),
        )
        root.layout(0, 0, width, height)
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(bitmap))

        val colors = buildSet {
            for (y in 0 until height step 8) {
                for (x in 0 until width step 8) add(bitmap.getPixel(x, y))
            }
        }
        assertTrue("Composed Inbox should render multiple visible colors", colors.size >= 4)
        assertTrue("Composed Inbox should render its approved background", Background.toArgb() in colors)
        assertNotEquals("Composed Inbox must not render as all black", setOf(android.graphics.Color.BLACK), colors)
        activity.finish()
    }
}

private fun renderedInboxState() = InboxUiState(
    reviewRequests = TabState(
        refreshedAtEpochMillis = 1_000_000L,
        items = listOf(
            InboxItem(
                nodeId = "reviews",
                repository = "ramo/ramo",
                number = 7,
                title = "Readable mobile reviews",
                url = "https://github.com/ramo/ramo/pull/7",
                author = "author",
                updatedAt = "1970-01-01T00:15:20Z",
                draft = false,
                additions = 42,
                deletions = 8,
                changedFiles = 20,
            ),
        ),
    ),
)
