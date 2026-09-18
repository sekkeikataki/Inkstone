package dev.inkstone.android.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.inkstone.android.data.NotebookDoc
import org.json.JSONObject

@Composable
fun SheetEditor(
    notebook: NotebookDoc,
    pageIndex: Int,
    revision: Int,
    onEditCell: (address: String, current: String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val pair = notebook.firstSpreadsheet(pageIndex)
    if (pair == null) {
        Box(modifier, contentAlignment = Alignment.Center) {
            Text("This page has no spreadsheet layer.", color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f))
        }
        return
    }
    val sheet = notebook.activeSheet(pair.second)
    val cols = NotebookDoc.displayCols(sheet)
    val rows = NotebookDoc.displayRows(sheet)
    val hScroll = rememberScrollState()
    val vScroll = rememberScrollState()
    Column(modifier.background(Color.White)) {
        Text(
            text = "${sheet.optString("name")} · $cols×$rows",
            modifier = Modifier.padding(12.dp),
            fontWeight = FontWeight.SemiBold,
        )
        Column(Modifier.verticalScroll(vScroll).horizontalScroll(hScroll).padding(bottom = 24.dp)) {
            Row {
                HeaderCell("", 48.dp)
                for (col in 0 until cols) {
                    HeaderCell(NotebookDoc.colName(col), 88.dp)
                }
            }
            for (row in 0 until rows) {
                Row {
                    HeaderCell("${row + 1}", 48.dp)
                    for (col in 0 until cols) {
                        val address = NotebookDoc.a1(col, row)
                        val value = cellDisplay(sheet, address)
                        Box(
                            Modifier
                                .width(88.dp)
                                .height(36.dp)
                                .border(0.5.dp, Color(0xFFC7D1C7))
                                .clickable { onEditCell(address, notebook.cellInput(sheet, address)) }
                                .padding(horizontal = 6.dp),
                            contentAlignment = Alignment.CenterStart,
                        ) {
                            Text(
                                text = value,
                                fontSize = 13.sp,
                                fontFamily = FontFamily.SansSerif,
                                maxLines = 1,
                            )
                        }
                    }
                }
            }
        }
    }
    revision.let { }
}

@Composable
private fun HeaderCell(label: String, width: androidx.compose.ui.unit.Dp) {
    Box(
        Modifier
            .width(width)
            .height(32.dp)
            .background(Color(0xFFE8EDE8))
            .border(0.5.dp, Color(0xFFC7D1C7)),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, fontSize = 12.sp, fontWeight = FontWeight.Medium)
    }
}

private fun cellDisplay(sheet: JSONObject, address: String): String {
    val cells = sheet.optJSONObject("cells") ?: return ""
    return cells.optJSONObject(address)?.optString("input").orEmpty()
}
