package dev.inkstone.android

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import dev.inkstone.android.data.NotebookDoc
import dev.inkstone.android.ui.EditorScreen
import dev.inkstone.android.ui.HomeScreen
import dev.inkstone.android.ui.InkstoneTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val initial = intent.notebookUri()?.let { loadUri(it) }
        setContent {
            InkstoneTheme {
                var notebook by remember { mutableStateOf(initial?.first) }
                var uri by remember { mutableStateOf(initial?.second) }
                var dirty by remember { mutableStateOf(false) }
                var error by remember { mutableStateOf<String?>(null) }

                val open = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { picked ->
                    if (picked != null) {
                        persist(picked)
                        val loaded = loadUri(picked)
                        if (loaded == null) {
                            error = "Could not read that file as an Inkstone notebook."
                        } else {
                            notebook = loaded.first
                            uri = loaded.second
                            dirty = false
                            error = null
                        }
                    }
                }
                val create = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/json")) { created ->
                    if (created != null) {
                        persist(created)
                        val blank = NotebookDoc.blank("Untitled notebook")
                        writeUri(created, blank.pretty())
                        notebook = blank
                        uri = created
                        dirty = false
                    }
                }

                val current = notebook
                if (current == null) {
                    HomeScreen(
                        error = error,
                        onOpen = { open.launch(arrayOf("application/json", "application/octet-stream", "*/*")) },
                        onNew = { create.launch("Untitled.inkstone") },
                    )
                } else {
                    EditorScreen(
                        notebook = current,
                        dirty = dirty,
                        onSave = {
                            val target = uri
                            if (target == null) {
                                create.launch("${current.title}.inkstone")
                            } else if (writeUri(target, current.pretty())) {
                                dirty = false
                                Toast.makeText(this, "Saved", Toast.LENGTH_SHORT).show()
                            } else {
                                error = "Save failed. Try Save as a new file from the system picker."
                            }
                        },
                        onClose = {
                            notebook = null
                            uri = null
                            dirty = false
                        },
                        onChanged = { dirty = true },
                    )
                }
            }
        }
    }

    private fun persist(uri: Uri) {
        try {
            contentResolver.takePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
            )
        } catch (_: SecurityException) {
        }
    }

    private fun loadUri(uri: Uri): Pair<NotebookDoc, Uri>? {
        return try {
            val text = contentResolver.openInputStream(uri)?.use { it.readBytes().toString(Charsets.UTF_8) }
                ?: return null
            NotebookDoc.parse(text) to uri
        } catch (error: Exception) {
            null
        }
    }

    private fun writeUri(uri: Uri, text: String): Boolean {
        return try {
            contentResolver.openOutputStream(uri, "wt")?.use { stream ->
                stream.write(text.toByteArray(Charsets.UTF_8))
            } != null
        } catch (_: Exception) {
            false
        }
    }
}

private fun Intent.notebookUri(): Uri? {
    return when (action) {
        Intent.ACTION_VIEW, Intent.ACTION_EDIT, Intent.ACTION_SEND -> data ?: clipData?.getItemAt(0)?.uri
        else -> data
    }
}
