package dev.inkstone.android.data

import org.json.JSONArray
import org.json.JSONObject
import java.util.Locale
import java.util.UUID
import kotlin.math.max
import kotlin.math.min

/** In-memory `.inkstone` notebook that mutates the JSON tree so unknown fields survive a save. */
class NotebookDoc(val root: JSONObject) {
    val format: String get() = root.optString("format")
    val version: Int get() = root.optInt("version")
    var title: String
        get() = root.optString("title").ifBlank { "Untitled notebook" }
        set(value) {
            root.put("title", value)
        }

    val pages: JSONArray
        get() = root.optJSONArray("pages") ?: JSONArray().also { root.put("pages", it) }

    val pageCount: Int get() = pages.length()

    fun page(index: Int): JSONObject = pages.getJSONObject(index.coerceIn(0, pageCount - 1))

    fun pageTitle(index: Int): String = page(index).optString("title").ifBlank { "Page ${index + 1}" }

    fun layers(pageIndex: Int): JSONArray =
        page(pageIndex).optJSONArray("layers") ?: JSONArray().also { page(pageIndex).put("layers", it) }

    fun assets(): JSONArray = root.optJSONArray("assets") ?: JSONArray().also { root.put("assets", it) }

    fun asset(id: String): JSONObject? {
        val assets = assets()
        for (i in 0 until assets.length()) {
            val asset = assets.getJSONObject(i)
            if (asset.optString("id") == id) return asset
        }
        return null
    }

    fun pretty(): String = root.toString(2) + "\n"

    fun firstNotesLayer(pageIndex: Int): JSONObject? {
        val layers = layers(pageIndex)
        for (i in 0 until layers.length()) {
            val layer = layers.getJSONObject(i)
            if (layer.optBoolean("visible", true) &&
                !layer.optBoolean("locked", false) &&
                !isSpreadsheet(layer)
            ) {
                return layer
            }
        }
        return null
    }

    fun ensureNotesLayer(pageIndex: Int): JSONObject {
        firstNotesLayer(pageIndex)?.let { return it }
        val layer = JSONObject()
            .put("id", UUID.randomUUID().toString())
            .put("name", "Notes")
            .put("visible", true)
            .put("locked", false)
            .put("elements", JSONArray())
        layers(pageIndex).put(layer)
        return layer
    }

    fun addPage(): Int {
        val index = pageCount
        pages.put(
            JSONObject()
                .put("id", UUID.randomUUID().toString())
                .put("title", "Page ${index + 1}")
                .put(
                    "canvas",
                    JSONObject()
                        .put("background", rgb(0.98, 0.98, 0.97))
                        .put("grid_spacing", 24.0)
                        .put("grid_visible", true),
                )
                .put(
                    "layers",
                    JSONArray().put(
                        JSONObject()
                            .put("id", UUID.randomUUID().toString())
                            .put("name", "Notes")
                            .put("visible", true)
                            .put("locked", false)
                            .put("elements", JSONArray()),
                    ),
                ),
        )
        return index
    }

    fun addSpreadsheetLayer(pageIndex: Int): JSONObject {
        val layer = JSONObject()
            .put("id", UUID.randomUUID().toString())
            .put("name", "Spreadsheet")
            .put("visible", true)
            .put("locked", false)
            .put("kind", "excel")
            .put("elements", JSONArray())
            .put(
                "spreadsheet",
                JSONObject()
                    .put("origin", JSONObject().put("x", 48.0).put("y", 36.0))
                    .put("active_sheet", 0)
                    .put("sheets", JSONArray().put(newSheet("Sheet1"))),
            )
        layers(pageIndex).put(layer)
        return layer
    }

    fun eraseAt(pageIndex: Int, x: Float, y: Float, radius: Float = 18f): Boolean {
        val layers = layers(pageIndex)
        for (i in layers.length() - 1 downTo 0) {
            val layer = layers.getJSONObject(i)
            if (!layer.optBoolean("visible", true) || layer.optBoolean("locked", false) || isSpreadsheet(layer)) {
                continue
            }
            val elements = layer.optJSONArray("elements") ?: continue
            for (j in elements.length() - 1 downTo 0) {
                val element = elements.getJSONObject(j)
                if (hitsElement(element, x, y, radius)) {
                    elements.remove(j)
                    return true
                }
            }
        }
        return false
    }

    fun firstSpreadsheet(pageIndex: Int): Pair<JSONObject, JSONObject>? {
        val layers = layers(pageIndex)
        for (i in 0 until layers.length()) {
            val layer = layers.getJSONObject(i)
            if (layer.optBoolean("visible", true) && isSpreadsheet(layer)) {
                val book = layer.optJSONObject("spreadsheet") ?: continue
                return layer to book
            }
        }
        return null
    }

    fun activeSheet(book: JSONObject): JSONObject {
        val sheets = book.optJSONArray("sheets") ?: JSONArray().also { book.put("sheets", it) }
        if (sheets.length() == 0) {
            val sheet = newSheet("Sheet1")
            sheets.put(sheet)
            return sheet
        }
        val index = book.optInt("active_sheet", 0).coerceIn(0, sheets.length() - 1)
        return sheets.getJSONObject(index)
    }

    fun setCell(pageIndex: Int, address: String, input: String) {
        val pair = firstSpreadsheet(pageIndex) ?: return
        val sheet = activeSheet(pair.second)
        val cells = sheet.optJSONObject("cells") ?: JSONObject().also { sheet.put("cells", it) }
        if (input.isBlank()) {
            cells.remove(address.uppercase(Locale.US))
        } else {
            val existing = cells.optJSONObject(address.uppercase(Locale.US)) ?: JSONObject()
            existing.put("input", input)
            cells.put(address.uppercase(Locale.US), existing)
        }
        growVisible(sheet, address)
    }

    fun cellInput(sheet: JSONObject, address: String): String {
        val cells = sheet.optJSONObject("cells") ?: return ""
        return cells.optJSONObject(address.uppercase(Locale.US))?.optString("input").orEmpty()
    }

    fun setText(pageIndex: Int, elementId: String, text: String) {
        forEachElement(pageIndex) { element ->
            if (element.optString("type") == "text" && element.optString("id") == elementId) {
                element.put("text", text)
            }
        }
    }

    fun addText(pageIndex: Int, x: Float, y: Float, text: String): String? {
        val layer = ensureNotesLayer(pageIndex)
        val elements = layer.optJSONArray("elements") ?: JSONArray().also { layer.put("elements", it) }
        val id = UUID.randomUUID().toString()
        elements.put(
            JSONObject()
                .put("type", "text")
                .put("id", id)
                .put("origin", JSONObject().put("x", x).put("y", y))
                .put("text", text)
                .put("font_size", 18.0)
                .put("color", rgb(0.10, 0.12, 0.16))
                .put("max_width", 420.0),
        )
        return id
    }

    fun addStroke(
        pageIndex: Int,
        points: List<Triple<Float, Float, Float>>,
        width: Float = 2.4f,
    ): String? {
        if (points.size < 2) return null
        val layer = ensureNotesLayer(pageIndex)
        val elements = layer.optJSONArray("elements") ?: JSONArray().also { layer.put("elements", it) }
        val id = UUID.randomUUID().toString()
        val jsonPoints = JSONArray()
        for (point in points) {
            jsonPoints.put(
                JSONObject()
                    .put("x", point.first.toDouble())
                    .put("y", point.second.toDouble())
                    .put("pressure", point.third.toDouble().coerceIn(0.05, 1.0)),
            )
        }
        elements.put(
            JSONObject()
                .put("type", "stroke")
                .put("id", id)
                .put("kind", "pen")
                .put(
                    "style",
                    JSONObject().put("color", rgb(0.10, 0.12, 0.16)).put("width", width.toDouble()),
                )
                .put("points", jsonPoints),
        )
        return id
    }

    fun hitText(pageIndex: Int, x: Float, y: Float): JSONObject? {
        var best: JSONObject? = null
        forEachElement(pageIndex) { element ->
            if (element.optString("type") != "text") return@forEachElement
            val origin = element.optJSONObject("origin") ?: return@forEachElement
            val ox = origin.optDouble("x").toFloat()
            val oy = origin.optDouble("y").toFloat()
            val size = element.optDouble("font_size", 18.0).toFloat()
            val text = element.optString("text")
            val width = element.optDouble("max_width", (text.length * size * 0.55).toDouble()).toFloat()
            val height = (text.lineSequence().count().coerceAtLeast(1) * size * 1.25f)
            if (x >= ox && x <= ox + width && y >= oy - size && y <= oy - size + height) {
                best = element
            }
        }
        return best
    }

    fun contentBounds(pageIndex: Int): Bounds {
        var minX = 0f
        var minY = 0f
        var maxX = 640f
        var maxY = 480f
        var found = false
        fun include(x: Float, y: Float) {
            if (!found) {
                minX = x
                minY = y
                maxX = x
                maxY = y
                found = true
            } else {
                minX = min(minX, x)
                minY = min(minY, y)
                maxX = max(maxX, x)
                maxY = max(maxY, y)
            }
        }
        forEachElement(pageIndex) { element ->
            when (element.optString("type")) {
                "stroke" -> {
                    val points = element.optJSONArray("points") ?: return@forEachElement
                    for (i in 0 until points.length()) {
                        val p = points.getJSONObject(i)
                        include(p.optDouble("x").toFloat(), p.optDouble("y").toFloat())
                    }
                }
                "text" -> {
                    val origin = element.optJSONObject("origin") ?: return@forEachElement
                    include(origin.optDouble("x").toFloat(), origin.optDouble("y").toFloat())
                }
                "shape", "media" -> {
                    val bounds = element.optJSONObject("bounds") ?: return@forEachElement
                    include(bounds.optDouble("x").toFloat(), bounds.optDouble("y").toFloat())
                    include(
                        (bounds.optDouble("x") + bounds.optDouble("width")).toFloat(),
                        (bounds.optDouble("y") + bounds.optDouble("height")).toFloat(),
                    )
                }
                "connector" -> {
                    val start = element.optJSONObject("start")?.optJSONObject("point")
                    val end = element.optJSONObject("end")?.optJSONObject("point")
                    if (start != null) include(start.optDouble("x").toFloat(), start.optDouble("y").toFloat())
                    if (end != null) include(end.optDouble("x").toFloat(), end.optDouble("y").toFloat())
                }
            }
        }
        firstSpreadsheet(pageIndex)?.let { (_, book) ->
            val origin = book.optJSONObject("origin")
            val ox = origin?.optDouble("x", 48.0)?.toFloat() ?: 48f
            val oy = origin?.optDouble("y", 36.0)?.toFloat() ?: 36f
            val sheet = activeSheet(book)
            val cols = displayCols(sheet)
            val rows = displayRows(sheet)
            include(ox, oy)
            include(ox + 36f + cols * 64f, oy + 26f + 22f + rows * 21f + 22f)
        }
        return Bounds(minX - 24f, minY - 24f, max(maxX - minX + 48f, 320f), max(maxY - minY + 48f, 240f))
    }

    private fun hitsElement(element: JSONObject, x: Float, y: Float, radius: Float): Boolean {
        return when (element.optString("type")) {
            "text" -> {
                val origin = element.optJSONObject("origin") ?: return false
                val ox = origin.optDouble("x").toFloat()
                val oy = origin.optDouble("y").toFloat()
                val size = element.optDouble("font_size", 18.0).toFloat()
                val text = element.optString("text")
                val width = element.optDouble("max_width", (text.length * size * 0.55).toDouble()).toFloat()
                val height = text.lineSequence().count().coerceAtLeast(1) * size * 1.25f
                x >= ox - radius && x <= ox + width + radius && y >= oy - size - radius && y <= oy - size + height + radius
            }
            "stroke" -> {
                val points = element.optJSONArray("points") ?: return false
                for (i in 0 until points.length() - 1) {
                    val a = points.getJSONObject(i)
                    val b = points.getJSONObject(i + 1)
                    if (segmentDistance(
                            x,
                            y,
                            a.optDouble("x").toFloat(),
                            a.optDouble("y").toFloat(),
                            b.optDouble("x").toFloat(),
                            b.optDouble("y").toFloat(),
                        ) <= radius
                    ) {
                        return true
                    }
                }
                false
            }
            "shape", "media" -> {
                val bounds = element.optJSONObject("bounds") ?: return false
                val left = bounds.optDouble("x").toFloat()
                val top = bounds.optDouble("y").toFloat()
                x >= left - radius &&
                    x <= left + bounds.optDouble("width").toFloat() + radius &&
                    y >= top - radius &&
                    y <= top + bounds.optDouble("height").toFloat() + radius
            }
            else -> false
        }
    }

    private fun segmentDistance(px: Float, py: Float, ax: Float, ay: Float, bx: Float, by: Float): Float {
        val dx = bx - ax
        val dy = by - ay
        val length = dx * dx + dy * dy
        if (length < 1e-4f) {
            val ex = px - ax
            val ey = py - ay
            return kotlin.math.sqrt(ex * ex + ey * ey)
        }
        val t = (((px - ax) * dx + (py - ay) * dy) / length).coerceIn(0f, 1f)
        val ex = px - (ax + dx * t)
        val ey = py - (ay + dy * t)
        return kotlin.math.sqrt(ex * ex + ey * ey)
    }

    private fun growVisible(sheet: JSONObject, address: String) {
        val parsed = parseA1(address) ?: return
        val cols = max(sheet.optInt("visible_cols", 10), parsed.first + 1)
        val rows = max(sheet.optInt("visible_rows", 10), parsed.second + 1)
        sheet.put("visible_cols", cols)
        sheet.put("visible_rows", rows)
    }

    private fun forEachElement(pageIndex: Int, visit: (JSONObject) -> Unit) {
        val layers = layers(pageIndex)
        for (i in 0 until layers.length()) {
            val layer = layers.getJSONObject(i)
            if (!layer.optBoolean("visible", true) || isSpreadsheet(layer)) continue
            val elements = layer.optJSONArray("elements") ?: continue
            for (j in 0 until elements.length()) {
                visit(elements.getJSONObject(j))
            }
        }
    }

    companion object {
        const val NOTEBOOK_FORMAT = "inkstone.notebook"
        const val LEGACY_FORMAT = "inkstone.document"

        fun blank(title: String = "Untitled notebook"): NotebookDoc {
            val pageId = UUID.randomUUID().toString()
            val layerId = UUID.randomUUID().toString()
            val root = JSONObject()
                .put("format", NOTEBOOK_FORMAT)
                .put("version", 2)
                .put("title", title)
                .put(
                    "pages",
                    JSONArray().put(
                        JSONObject()
                            .put("id", pageId)
                            .put("title", "Page 1")
                            .put(
                                "canvas",
                                JSONObject()
                                    .put("background", rgb(0.98, 0.98, 0.97))
                                    .put("grid_spacing", 24.0)
                                    .put("grid_visible", true),
                            )
                            .put(
                                "layers",
                                JSONArray().put(
                                    JSONObject()
                                        .put("id", layerId)
                                        .put("name", "Notes")
                                        .put("visible", true)
                                        .put("locked", false)
                                        .put("elements", JSONArray()),
                                ),
                            ),
                    ),
                )
                .put("assets", JSONArray())
            return NotebookDoc(root)
        }

        fun parse(text: String): NotebookDoc {
            val root = JSONObject(text)
            return when (root.optString("format")) {
                NOTEBOOK_FORMAT -> NotebookDoc(root)
                LEGACY_FORMAT -> fromLegacy(root)
                else -> throw IllegalArgumentException("unsupported document format: ${root.optString("format")}")
            }
        }

        fun isSpreadsheet(layer: JSONObject): Boolean = layer.optString("kind") == "excel"

        fun displayCols(sheet: JSONObject): Int {
            var used = 0
            val cells = sheet.optJSONObject("cells")
            if (cells != null) {
                val keys = cells.keys()
                while (keys.hasNext()) {
                    val addr = parseA1(keys.next()) ?: continue
                    used = max(used, addr.first + 1)
                }
            }
            return max(sheet.optInt("visible_cols", 10), max(used, 10))
        }

        fun displayRows(sheet: JSONObject): Int {
            var used = 0
            val cells = sheet.optJSONObject("cells")
            if (cells != null) {
                val keys = cells.keys()
                while (keys.hasNext()) {
                    val addr = parseA1(keys.next()) ?: continue
                    used = max(used, addr.second + 1)
                }
            }
            return max(sheet.optInt("visible_rows", 10), max(used, 10))
        }

        fun colName(col: Int): String {
            var n = col + 1
            val chars = ArrayDeque<Char>()
            while (n > 0) {
                n -= 1
                chars.addFirst(('A'.code + n % 26).toChar())
                n /= 26
            }
            return chars.joinToString("")
        }

        fun a1(col: Int, row: Int): String = "${colName(col)}${row + 1}"

        fun parseA1(input: String): Pair<Int, Int>? {
            val match = Regex("^\\$?([A-Za-z]+)\\$?([0-9]+)$").matchEntire(input.trim()) ?: return null
            val col = parseCol(match.groupValues[1]) ?: return null
            val row = match.groupValues[2].toInt() - 1
            if (row < 0) return null
            return col to row
        }

        fun parseCol(name: String): Int? {
            var n = 0
            for (ch in name.uppercase(Locale.US)) {
                if (ch !in 'A'..'Z') return null
                n = n * 26 + (ch - 'A' + 1)
            }
            return if (n == 0) null else n - 1
        }

        fun rgb(r: Double, g: Double, b: Double, a: Double = 1.0): JSONObject =
            JSONObject().put("red", r).put("green", g).put("blue", b).put("alpha", a)

        private fun fromLegacy(document: JSONObject): NotebookDoc {
            val root = JSONObject()
                .put("format", NOTEBOOK_FORMAT)
                .put("version", 2)
                .put("title", document.optString("title").ifBlank { "Imported note" })
                .put(
                    "pages",
                    JSONArray().put(
                        JSONObject()
                            .put("id", UUID.randomUUID().toString())
                            .put("title", document.optString("title").ifBlank { "Page 1" })
                            .put(
                                "canvas",
                                document.optJSONObject("canvas")
                                    ?: JSONObject()
                                        .put("background", rgb(0.98, 0.98, 0.97))
                                        .put("grid_spacing", 24.0)
                                        .put("grid_visible", true),
                            )
                            .put(
                                "layers",
                                JSONArray().put(
                                    JSONObject()
                                        .put("id", UUID.randomUUID().toString())
                                        .put("name", "Imported notes")
                                        .put("visible", true)
                                        .put("locked", false)
                                        .put("elements", document.optJSONArray("elements") ?: JSONArray()),
                                ),
                            ),
                    ),
                )
                .put("assets", JSONArray())
            return NotebookDoc(root)
        }

        private fun newSheet(name: String): JSONObject =
            JSONObject()
                .put("name", name)
                .put("cells", JSONObject())
                .put("visible_cols", 10)
                .put("visible_rows", 10)
    }
}

data class Bounds(val x: Float, val y: Float, val width: Float, val height: Float)

fun JSONObject.colorArgb(): Int {
    val r = (optDouble("red", 0.1) * 255).toInt().coerceIn(0, 255)
    val g = (optDouble("green", 0.12) * 255).toInt().coerceIn(0, 255)
    val b = (optDouble("blue", 0.16) * 255).toInt().coerceIn(0, 255)
    val a = (optDouble("alpha", 1.0) * 255).toInt().coerceIn(0, 255)
    return (a shl 24) or (r shl 16) or (g shl 8) or b
}

