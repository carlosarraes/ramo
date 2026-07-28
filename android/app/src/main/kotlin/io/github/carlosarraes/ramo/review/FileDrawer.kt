package io.github.carlosarraes.ramo.review

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

@Composable
fun FileDrawer(files: List<FileSummaryUi>, selected: Int, onSelect: (Int) -> Unit) {
    Column {
        Text("Files", Modifier.padding(16.dp), style = MaterialTheme.typography.titleLarge)
        files.forEachIndexed { index, file ->
            Column(
                Modifier.fillMaxWidth().clickable { onSelect(index) }.padding(horizontal = 16.dp, vertical = 10.dp),
            ) {
                Text(file.path, fontWeight = if (index == selected) FontWeight.Bold else FontWeight.Normal)
                Text("+${file.additions}  −${file.deletions}${if (file.viewed) "  Viewed" else ""}")
            }
        }
    }
}
