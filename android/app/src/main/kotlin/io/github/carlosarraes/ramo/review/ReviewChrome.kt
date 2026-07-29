package io.github.carlosarraes.ramo.review

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ReviewTopBar(
    fileName: String,
    currentFile: Int,
    fileCount: Int,
    onBack: () -> Unit,
    onFiles: () -> Unit,
) {
    TopAppBar(
        navigationIcon = {
            TextButton(onClick = onBack, modifier = Modifier.heightIn(min = 48.dp)) {
                Text("Back")
            }
        },
        title = {
            Text(
                text = fileName,
                modifier = Modifier.testTag("review-top-title"),
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        },
        actions = {
            TextButton(onClick = onFiles, modifier = Modifier.heightIn(min = 48.dp)) {
                Text("$currentFile / $fileCount")
            }
        },
        colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.surface),
    )
}

@Composable
fun ReviewBottomNavigation(
    canPrevious: Boolean,
    canNext: Boolean,
    onPrevious: () -> Unit,
    onFinish: () -> Unit,
    onNext: () -> Unit,
) {
    Surface(
        modifier = Modifier.windowInsetsPadding(WindowInsets.navigationBars),
        tonalElevation = 3.dp,
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .testTag("review-bottom-nav")
                .padding(horizontal = 8.dp, vertical = 6.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TextButton(onClick = onPrevious, enabled = canPrevious) { Text("Previous file") }
            Button(onClick = onFinish) { Text("Finish") }
            TextButton(onClick = onNext, enabled = canNext) { Text("Next file") }
        }
    }
}
