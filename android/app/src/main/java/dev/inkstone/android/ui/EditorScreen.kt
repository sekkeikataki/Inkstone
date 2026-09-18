package dev.inkstone.android.ui

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.PrimaryTabRow
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
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
    var tool by remember { mutableStateOf(EditTool.Ink) }
    var revision by remember { mutableIntStateOf(0) }
    var editCell by remember { mutableStateOf<Pair<String, String>?>(null) }
    var editText by remember { mutableStateOf<Triple<String?, Float, Float>?>(null) }
    var textBuffer by remember { mutableStateOf("") }

    fun bump() {
        revision += 1
        onChanged()
    }

    val hint = when (tool) {
        EditTool.Pan -> "Drag to move the page · pinch to zoom"
        EditTool.Ink -> "Draw with your finger"
        EditTool.Erase -> "Drag over ink or a note to delete it"
        EditTool.Text -> "Tap the page to add or edit a note"
        EditTool.Cells -> "Tap a cell to type a value or =formula"
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(if (dirty) "${notebook.title} · edited" else notebook.title) },
                navigationIcon = {
                    TextButton(onClick = onClose) { Text("Close") }
                },
                actions = {
                    Button(onClick = onSave) { Text(if (dirty) "Save" else "Saved") }
                },
            )
        },
        bottomBar = {
            Surface(tonalElevation = 3.dp, shadowElevation = 6.dp) {
                Column(Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 8.dp)) {
                    Text(
                        hint,
                        style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.primary,
                        modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
                    )
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .horizontalScroll(rememberScrollState()),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        ToolButton("Pan", tool == EditTool.Pan) { tool = EditTool.Pan }
                        ToolButton("Draw", tool == EditTool.Ink) { tool = EditTool.Ink }
                        ToolButton("Erase", tool == EditTool.Erase) { tool = EditTool.Erase }
                        ToolButton("Text", tool == EditTool.Text) { tool = EditTool.Text }
                        ToolButton("Cells", tool == EditTool.Cells) {
                            if (notebook.firstSpreadsheet(pageIndex) == null) {
                                notebook.addSpreadsheetLayer(pageIndex)
                                bump()
                            }
                            tool = EditTool.Cells
                        }
                        OutlinedButton(onClick = {
                            pageIndex = notebook.addPage()
                            tool = EditTool.Ink
                            bump()
                        }, modifier = Modifier.height(48.dp)) { Text("+ Page") }
                        OutlinedButton(onClick = {
                            notebook.addSpreadsheetLayer(pageIndex)
                            tool = EditTool.Cells
                            bump()
                        }, modifier = Modifier.height(48.dp)) { Text("+ Sheet") }
                    }
                }
            }
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            if (notebook.pageCount > 1) {
                PrimaryTabRow(selectedTabIndex = pageIndex.coerceAtMost(notebook.pageCount - 1)) {
                    for (index in 0 until notebook.pageCount) {
                        Tab(
                            selected = pageIndex == index,
                            onClick = { pageIndex = index },
                            text = { Text(notebook.pageTitle(index)) },
                        )
                    }
                }
            }
            if (tool == EditTool.Cells) {
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
                    onErase = { x, y ->
                        if (notebook.eraseAt(pageIndex, x, y)) bump()
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
private fun ToolButton(label: String, selected: Boolean, onClick: () -> Unit) {
    if (selected) {
        FilledTonalButton(onClick = onClick, modifier = Modifier.height(48.dp)) { Text(label) }
    } else {
        OutlinedButton(onClick = onClick, modifier = Modifier.height(48.dp)) { Text(label) }
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
                "Open a notebook or start a new one. The bottom bar has Pan, Draw, Erase, Text, and Cells so you can edit the file on your phone and Save it back.",
                style = MaterialTheme.typography.bodyMedium,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Button(onClick = onOpen) { Text("Open notebook") }
                OutlinedButton(onClick = onNew) { Text("New notebook") }
            }
            if (error != null) {
                Text(error, color = MaterialTheme.colorScheme.error)
            }
        }
    }
}
