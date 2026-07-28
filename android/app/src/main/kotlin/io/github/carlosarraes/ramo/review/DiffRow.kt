package io.github.carlosarraes.ramo.review

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.ScrollState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

@Composable
fun DiffRow(
    row: DiffRowUi,
    horizontalScroll: ScrollState,
    codeSize: Int,
    onComment: (DiffRowUi) -> Unit,
    onExpand: (DiffRowUi) -> Unit,
) {
    val background = when (row.kind) {
        LineKindUi.Addition -> Color(0x3328A745)
        LineKindUi.Deletion -> Color(0x33F7768E)
        LineKindUi.Hunk -> Color(0x332E3C64)
        LineKindUi.Context -> Color.Transparent
    }
    Row(Modifier.fillMaxWidth().background(background)) {
        Text(
            text = "${row.oldLine ?: ""} ${row.newLine ?: ""}",
            modifier = Modifier
                .width(72.dp)
                .clickable(enabled = row.commentable || row.key.contains(":gap:")) {
                    if (row.commentable) onComment(row) else onExpand(row)
                }
                .padding(horizontal = 6.dp, vertical = 2.dp),
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            fontFamily = FontFamily.Monospace,
            fontSize = codeSize.sp,
            maxLines = 1,
        )
        Text(
            text = buildAnnotatedString {
                row.spans.forEach { span ->
                    withStyle(
                        SpanStyle(
                            color = Color(span.color.toULong()),
                            fontWeight = if (span.bold) FontWeight.Bold else FontWeight.Normal,
                            fontStyle = if (span.italic) FontStyle.Italic else FontStyle.Normal,
                            textDecoration = if (span.underline) TextDecoration.Underline else null,
                        ),
                    ) { append(span.text) }
                }
            },
            modifier = Modifier.horizontalScroll(horizontalScroll).padding(horizontal = 6.dp, vertical = 2.dp),
            fontFamily = FontFamily.Monospace,
            fontSize = codeSize.sp,
            softWrap = false,
            maxLines = 1,
        )
    }
}
