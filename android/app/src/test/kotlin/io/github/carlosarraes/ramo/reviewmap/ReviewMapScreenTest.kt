package io.github.carlosarraes.ramo.reviewmap

import androidx.compose.ui.test.assertHasClickAction
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onAllNodesWithText
import io.github.carlosarraes.ramo.ui.theme.RamoAppSurface
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ReviewMapScreenTest {
    @get:Rule val compose = createComposeRule()

    @Test
    fun enrichedScreenShowsExactTotalsAndStartTarget() {
        val file = ReviewMapFileUi("f1", "src/billing/proration.ts", 414, 60, ReviewFileKindUi.Authored, summary = "Core billing path", recommendedOrder = 1)
        val group = ReviewMapGroupUi("g1", "src/billing/", ReviewFileKindUi.Authored, listOf("f1"), 414, 60, false, "Core billing path")
        val map = ReviewMapUi("owner/repo", 7, "base", "head", 414, 60, listOf(group), listOf(file), "qwen2.5-coder:7b")
        compose.setContent {
            RamoAppSurface {
                ReviewMapScreen(
                    ReviewMapUiState(false, map, ReviewMapPhase.Enriched, setOf("g1")),
                    ReviewMapCallbacks({}, {}, {}, {}, {}),
                )
            }
        }

        compose.onAllNodesWithText("+414")[0].assertIsDisplayed()
        compose.onAllNodesWithText("−60")[0].assertIsDisplayed()
        compose.onAllNodesWithText("Core billing path", substring = true)[0].assertIsDisplayed()
        compose.onNodeWithText("Start with proration.ts").assertHasClickAction()
    }
}
