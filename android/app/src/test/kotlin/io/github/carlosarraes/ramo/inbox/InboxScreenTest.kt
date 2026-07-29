package io.github.carlosarraes.ramo.inbox

import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.assertHasClickAction
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import io.github.carlosarraes.ramo.ui.theme.RamoAppSurface
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class InboxScreenTest {
    @get:Rule val compose = createComposeRule()

    @Test
    fun queueShowsFileCountWithoutAccountTextOrCards() {
        compose.setContent {
            RamoAppSurface {
                InboxScreen(
                    login = "carlosarraes",
                    state = inboxState(changedFiles = 20),
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

        compose.onNodeWithText("20 files").assertIsDisplayed()
        compose.onNodeWithText("Review requests 1").assertIsDisplayed()
        compose.onNodeWithText("Your PRs 0").assertIsDisplayed()
        compose.onNodeWithText("Updated 16m ago").assertIsDisplayed()
        compose.onNodeWithText("Review requested").assertIsDisplayed()
        compose.onNodeWithText("@carlosarraes · Sign out").assertDoesNotExist()
        compose.onNodeWithTag("inbox-row-reviews").assertHasClickAction()
    }
}

private fun inboxState(changedFiles: Long) = InboxUiState(
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
                changedFiles = changedFiles,
            ),
        ),
    ),
)
