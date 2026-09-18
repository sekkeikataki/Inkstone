package dev.inkstone.android.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.PrimaryTabRow
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.unit.dp
import dev.inkstone.android.data.NotebookDoc

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun EditorScreen(
    notebook: NotebookDoc,
    dirty: Boolean,
    onSave: () -> Unit,
    onClose: () -> Unit,
    onChanged: () -> Unit,
) {
    var pageIndex by remember { mutableIntStateOf(0) }
    var mode by remember { mutableStateOf(if (notebook.firstSpreadsheet(0) != null) 1 else 0) }
    var tool by remember { mutableStateOf(EditTool.View) }
    var revision by remember { mutableIntStateOf(0) }
    var editCell by remember { mutableStateOf<Pair<String, String>?>(null) }
    var editText by remember { mutableStateOf<Triple<String?, Float, Float>?>(null) }
    var textBuffer by remember { mutableStateOf("") }

    fun bump() {
        revision += 1
        onChanged()
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(if (dirty) "${notebook.title} · edited" else notebook.title) },
                navigationIcon = {
                    TextButton(onClick = onClose) { Text("Close") }
                },
                actions = {
                    TextButton(onClick = onSave, enabled = dirty) { Text("Save") }
                },
            )
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            if (notebook.pageCount > 1) {
                PrimaryTabRow(selectedTabIndex = pageIndex.coerceAtMost(notebook.pageCount - 1)) {
                    for (index in 0 until notebook.pageCount) {
                        Tab(
                            selected = pageIndex == index,
                            onClick = {
                                pageIndex = index
                                mode = if (notebook.firstSpreadsheet(index) != null) mode else 0
                            },
                            text = { Text(notebook.pageTitle(index)) },
                        )
                    }
                }
            }
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                FilterChip(selected = mode == 0, onClick = { mode = 0 }, label = { Text("Page") })
                if (notebook.firstSpreadsheet(pageIndex) != null) {
                    FilterChip(selected = mode == 1, onClick = { mode = 1 }, label = { Text("Cells") })
                }
                if (mode == 0) {
                    FilterChip(selected = tool == EditTool.View, onClick = { tool = EditTool.View }, label = { Text("View") })
                    FilterChip(selected = tool == EditTool.Ink, onClick = { tool = EditTool.Ink }, label = { Text("Ink") })
                    FilterChip(selected = tool == EditTool.Text, onClick = { tool = EditTool.Text }, label = { Text("Text") })
                }
            }
            if (mode == 1) {
                SheetEditor(
                    notebook = notebook,
                    pageIndex = pageIndex,
                    revision = revision,
                    onEditCell = { address, current ->
                        editCell = address to current
                        textBuffer = current
                    },
                    modifier = Modifier.fillMaxSize(),
                )
            } else {
                PageCanvas(
                    notebook = notebook,
                    pageIndex = pageIndex,
                    tool = tool,
                    revision = revision,
                    onInk = { points ->
                        notebook.addStroke(pageIndex, points)
                        bump()
                    },
                    onTextTap = { id, x, y ->
                        editText = Triple(id, x, y)
                        textBuffer = if (id != null) {
                            notebook.hitText(pageIndex, x, y)?.optString("text").orEmpty()
                        } else {
                            ""
                        }
                    },
                    modifier = Modifier.fillMaxSize(),
                )
            }
        }
    }

    editCell?.let { (address, _) ->
        AlertDialog(
            onDismissRequest = { editCell = null },
            title = { Text(address) },
            text = {
                OutlinedTextField(
                    value = textBuffer,
                    onValueChange = { textBuffer = it },
                    label = { Text("Value or =formula") },
                    singleLine = true,
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    notebook.setCell(pageIndex, address, textBuffer)
                    editCell = null
                    bump()
                }) { Text("OK") }
            },
            dismissButton = { TextButton(onClick = { editCell = null }) { Text("Cancel") } },
        )
    }

    editText?.let { (id, x, y) ->
        AlertDialog(
            onDismissRequest = { editText = null },
            title = { Text(if (id == null) "Add note" else "Edit note") },
            text = {
                OutlinedTextField(
                    value = textBuffer,
                    onValueChange = { textBuffer = it },
                    label = { Text("Text") },
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    if (id != null) notebook.setText(pageIndex, id, textBuffer)
                    else if (textBuffer.isNotBlank()) notebook.addText(pageIndex, x, y, textBuffer)
                    editText = null
                    bump()
                }) { Text("OK") }
            },
            dismissButton = { TextButton(onClick = { editText = null }) { Text("Cancel") } },
        )
    }
}

@Composable
fun HomeScreen(
    error: String?,
    onOpen: () -> Unit,
    onNew: () -> Unit,
) {
    Scaffold { padding ->
        Column(
            Modifier.fillMaxSize().padding(padding).padding(24.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Text("Inkstone", style = MaterialTheme.typography.headlineMedium)
            Text(
                "Open a Linux `.inkstone` notebook to view the canvas, sketch a quick note, or edit spreadsheet cells. Saves write the same v2 JSON the desktop app uses.",
                style = MaterialTheme.typography.bodyMedium,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                androidx.compose.material3.Button(onClick = onOpen) { Text("Open notebook") }
                androidx.compose.material3.OutlinedButton(onClick = onNew) { Text("New notebook") }
            }
            if (error != null) {
                Text(error, color = MaterialTheme.colorScheme.error)
            }
        }
    }
}
