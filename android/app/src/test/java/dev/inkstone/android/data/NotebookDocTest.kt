package dev.inkstone.android.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class NotebookDocTest {
    @Test
    fun blankNotebookIsV2() {
        val notebook = NotebookDoc.blank("Phone")
        assertEquals(NotebookDoc.NOTEBOOK_FORMAT, notebook.format)
        assertEquals(2, notebook.version)
        assertEquals("Phone", notebook.title)
        assertEquals(1, notebook.pageCount)
    }

    @Test
    fun cellAndStrokeRoundTrip() {
        val notebook = NotebookDoc.blank()
        val sheetLayer = org.json.JSONObject()
            .put("id", "11111111-1111-1111-1111-111111111111")
            .put("name", "Budget")
            .put("visible", true)
            .put("locked", false)
            .put("kind", "excel")
            .put("elements", org.json.JSONArray())
            .put(
                "spreadsheet",
                org.json.JSONObject()
                    .put("origin", org.json.JSONObject().put("x", 48).put("y", 36))
                    .put("active_sheet", 0)
                    .put(
                        "sheets",
                        org.json.JSONArray().put(
                            org.json.JSONObject()
                                .put("name", "Sheet1")
                                .put("cells", org.json.JSONObject())
                                .put("visible_cols", 10)
                                .put("visible_rows", 10),
                        ),
                    ),
            )
        notebook.layers(0).put(sheetLayer)
        notebook.setCell(0, "A1", "10")
        notebook.setCell(0, "B1", "=A1*2")
        notebook.addStroke(0, listOf(Triple(10f, 10f, 0.8f), Triple(40f, 28f, 0.6f)))
        notebook.addText(0, 20f, 80f, "Hello")

        val again = NotebookDoc.parse(notebook.pretty())
        val sheet = again.activeSheet(again.firstSpreadsheet(0)!!.second)
        assertEquals("10", again.cellInput(sheet, "A1"))
        assertEquals("=A1*2", again.cellInput(sheet, "B1"))
        assertEquals(2, again.firstNotesLayer(0)!!.getJSONArray("elements").length())
        assertTrue(again.pretty().contains("\"kind\": \"excel\""))
    }

    @Test
    fun a1AddressesMatchExcel() {
        assertEquals("A1", NotebookDoc.a1(0, 0))
        assertEquals("Z1", NotebookDoc.a1(25, 0))
        assertEquals("AA2", NotebookDoc.a1(26, 1))
        assertEquals(0 to 0, NotebookDoc.parseA1("A1"))
        assertEquals(26 to 1, NotebookDoc.parseA1("AA2"))
    }

    @Test
    fun legacyV1Migrates() {
        val legacy = """
            {
              "format": "inkstone.document",
              "version": 1,
              "title": "Old note",
              "canvas": {
                "background": {"red": 0.98, "green": 0.98, "blue": 0.97, "alpha": 1.0},
                "grid_spacing": 24.0,
                "grid_visible": true
              },
              "elements": []
            }
        """.trimIndent()
        val notebook = NotebookDoc.parse(legacy)
        assertEquals(NotebookDoc.NOTEBOOK_FORMAT, notebook.format)
        assertEquals("Old note", notebook.title)
        assertEquals(1, notebook.pageCount)
    }
}
