package io.github.carlosarraes.ramo.settings

import androidx.compose.ui.test.assertHasClickAction
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import io.github.carlosarraes.ramo.notifications.NotificationPermissionSheet
import io.github.carlosarraes.ramo.ui.theme.RamoAppSurface
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class SettingsScreenTest {
    @get:Rule val compose = createComposeRule()

    @Test
    fun settingsOwnIdentityCodeSizeAndNotifications() {
        compose.setContent {
            RamoAppSurface {
                SettingsScreen(
                    login = "carlosarraes",
                    codeSize = 13,
                    notificationsGranted = false,
                    onCodeSize = {},
                    onEnableNotifications = {},
                    onBack = {},
                    onSignOut = {},
                )
            }
        }

        compose.onNodeWithText("@carlosarraes").assertIsDisplayed()
        compose.onNodeWithText("Code size · 13").assertIsDisplayed()
        compose.onNodeWithText("Enable notifications").assertHasClickAction()
    }

    @Test
    fun notificationSheetHasPersistentDismissChoice() {
        compose.setContent {
            RamoAppSurface { NotificationPermissionSheet(onEnable = {}, onNotNow = {}) }
        }

        compose.onNodeWithText("Enable notifications").assertHasClickAction()
        compose.onNodeWithText("Not now").assertHasClickAction()
    }
}
