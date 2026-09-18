package dev.inkstone.android.ui

import android.graphics.BitmapFactory
import android.util.Base64
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.input.pointer.pointerInput
import dev.inkstone.android.data.NotebookDoc
import dev.inkstone.android.data.colorArgb
import org.json.JSONObject
import kotlin.math.max
import kotlin.math.roundToInt

enum class EditTool { View, Ink, Text }

@Composable
fun PageCanvas(
    notebook: NotebookDoc,
    pageIndex: Int,
    tool: EditTool,
    revision: Int,
    onInk: (List<Triple<Float, Float, Float>>) -> Unit,
    onTextTap: (id: String?, x: Float, y: Float) -> Unit,
    modifier: Modifier = Modifier,
) {
    val bounds = remember(notebook, pageIndex, revision) { notebook.contentBounds(pageIndex) }
    var scale by remember(pageIndex) { mutableFloatStateOf(1f) }
    var pan by remember(pageIndex) { mutableStateOf(Offset(-bounds.x + 24f, -bounds.y + 24f)) }
    var ink by remember { mutableStateOf<List<Offset>>(emptyList()) }

    fun toWorld(position: Offset): Offset =
        Offset((position.x - pan.x) / scale, (position.y - pan.y) / scale)

    val gestures = when (tool) {
        EditTool.View -> Modifier.pointerInput(pageIndex, revision) {
            detectTransformGestures { _, drag, zoom, _ ->
                scale = (scale * zoom).coerceIn(0.2f, 8f)
                pan += drag
            }
        }
        EditTool.Ink -> Modifier.pointerInput(pageIndex, revision) {
            detectDragGestures(
                onDragStart = { start -> ink = listOf(toWorld(start)) },
                onDrag = { change, _ ->
                    change.consume()
                    ink = ink + toWorld(change.position)
                },
                onDragEnd = {
                    if (ink.size >= 2) {
                        onInk(ink.map { Triple(it.x, it.y, 0.7f) })
                    }
                    ink = emptyList()
                },
                onDragCancel = { ink = emptyList() },
            )
        }
        EditTool.Text -> Modifier.pointerInput(pageIndex, revision) {
            detectTapGestures { tap ->
                val world = toWorld(tap)
                val hit = notebook.hitText(pageIndex, world.x, world.y)
                onTextTap(hit?.optString("id"), world.x, world.y)
            }
        }
    }

    Canvas(modifier.fillMaxSize().then(gestures)) {
        val page = notebook.page(pageIndex)
        val canvas = page.optJSONObject("canvas")
        val background = canvas?.optJSONObject("background")
        drawRect(Color(background?.colorArgb() ?: 0xFFFAFAF7.toInt()))
        withTransform({
            translate(pan.x, pan.y)
            scale(scale, scale)
        }) {
            if (canvas?.optBoolean("grid_visible", true) == true) {
                drawGrid(canvas.optDouble("grid_spacing", 24.0).toFloat(), bounds)
            }
            val layers = notebook.layers(pageIndex)
            for (i in 0 until layers.length()) {
                val layer = layers.getJSONObject(i)
                if (!layer.optBoolean("visible", true)) continue
                if (NotebookDoc.isSpreadsheet(layer)) {
                    layer.optJSONObject("spreadsheet")?.let { drawSpreadsheet(it) }
                    continue
                }
                val elements = layer.optJSONArray("elements") ?: continue
                for (j in 0 until elements.length()) {
                    drawElement(elements.getJSONObject(j), notebook)
                }
            }
            if (ink.size >= 2) {
                val path = Path()
                path.moveTo(ink.first().x, ink.first().y)
                ink.drop(1).forEach { path.lineTo(it.x, it.y) }
                drawPath(
                    path,
                    Color(0xFF1A1F29),
                    style = Stroke(width = 2.4f, cap = StrokeCap.Round, join = StrokeJoin.Round),
                )
            }
        }
    }
}

private fun DrawScope.drawGrid(spacing: Float, bounds: dev.inkstone.android.data.Bounds) {
    val step = max(spacing, 8f)
    val left = bounds.x - 200f
    val top = bounds.y - 200f
    val right = bounds.x + bounds.width + 400f
    val bottom = bounds.y + bounds.height + 400f
    val color = Color(0x221A1F29)
    var x = (left / step).toInt() * step
    while (x <= right) {
        drawLine(color, Offset(x, top), Offset(x, bottom), 1f)
        x += step
    }
    var y = (top / step).toInt() * step
    while (y <= bottom) {
        drawLine(color, Offset(left, y), Offset(right, y), 1f)
        y += step
    }
}

private fun DrawScope.drawElement(element: JSONObject, notebook: NotebookDoc) {
    when (element.optString("type")) {
        "stroke" -> drawStroke(element)
        "text" -> drawTextNote(element)
        "shape" -> drawShape(element)
        "connector" -> drawConnector(element)
        "media" -> drawMedia(element, notebook)
    }
}

private fun DrawScope.drawStroke(element: JSONObject) {
    val points = element.optJSONArray("points") ?: return
    if (points.length() < 2) return
    val style = element.optJSONObject("style")
    val color = Color(style?.optJSONObject("color")?.colorArgb() ?: 0xFF1A1F29.toInt())
    val width = style?.optDouble("width", 2.4)?.toFloat() ?: 2.4f
    val path = Path()
    val first = points.getJSONObject(0)
    path.moveTo(first.optDouble("x").toFloat(), first.optDouble("y").toFloat())
    for (i in 1 until points.length()) {
        val point = points.getJSONObject(i)
        path.lineTo(point.optDouble("x").toFloat(), point.optDouble("y").toFloat())
    }
    val pressure = ((first.optDouble("pressure", 0.7) +
        points.getJSONObject(points.length() - 1).optDouble("pressure", 0.7)) / 2.0)
        .toFloat()
        .coerceIn(0.12f, 1f)
    drawPath(path, color, style = Stroke(width = width * pressure, cap = StrokeCap.Round, join = StrokeJoin.Round))
}

private fun DrawScope.drawTextNote(element: JSONObject) {
    val origin = element.optJSONObject("origin") ?: return
    val paint = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply {
        color = element.optJSONObject("color")?.colorArgb() ?: 0xFF1A1F29.toInt()
        textSize = element.optDouble("font_size", 18.0).toFloat()
    }
    val x = origin.optDouble("x").toFloat()
    var y = origin.optDouble("y").toFloat()
    element.optString("text").split('\n').forEach { line ->
        drawContext.canvas.nativeCanvas.drawText(line, x, y, paint)
        y += paint.textSize * 1.25f
    }
}

private fun DrawScope.drawShape(element: JSONObject) {
    val bounds = element.optJSONObject("bounds") ?: return
    val x = bounds.optDouble("x").toFloat()
    val y = bounds.optDouble("y").toFloat()
    val w = bounds.optDouble("width").toFloat()
    val h = bounds.optDouble("height").toFloat()
    val style = element.optJSONObject("style")
    val stroke = Color(style?.optJSONObject("color")?.colorArgb() ?: 0xFF1A1F29.toInt())
    val width = style?.optDouble("width", 1.8)?.toFloat() ?: 1.8f
    element.optJSONObject("fill")?.let { fill ->
        drawRect(Color(fill.colorArgb()), Offset(x, y), Size(w, h))
    }
    when (element.optString("kind")) {
        "ellipse" -> drawOval(color = stroke, topLeft = Offset(x, y), size = Size(w, h), style = Stroke(width))
        else -> drawRect(color = stroke, topLeft = Offset(x, y), size = Size(w, h), style = Stroke(width))
    }
    val label = element.optString("label")
    if (label.isNotBlank()) {
        val paint = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply {
            color = stroke.hashCode()
            this.color = android.graphics.Color.argb(
                (stroke.alpha * 255).roundToInt(),
                (stroke.red * 255).roundToInt(),
                (stroke.green * 255).roundToInt(),
                (stroke.blue * 255).roundToInt(),
            )
            textSize = 12f
        }
        drawContext.canvas.nativeCanvas.drawText(label, x + 6f, y + h / 2f, paint)
    }
}

private fun DrawScope.drawConnector(element: JSONObject) {
    val style = element.optJSONObject("style")
    val color = Color(style?.optJSONObject("color")?.colorArgb() ?: 0xFF1A1F29.toInt())
    val width = style?.optDouble("width", 1.6)?.toFloat() ?: 1.6f
    val start = element.optJSONObject("start")?.optJSONObject("point") ?: return
    val end = element.optJSONObject("end")?.optJSONObject("point") ?: return
    val path = Path()
    path.moveTo(start.optDouble("x").toFloat(), start.optDouble("y").toFloat())
    val route = element.optJSONArray("route")
    if (route != null) {
        for (i in 0 until route.length()) {
            val point = route.getJSONObject(i)
            path.lineTo(point.optDouble("x").toFloat(), point.optDouble("y").toFloat())
        }
    }
    path.lineTo(end.optDouble("x").toFloat(), end.optDouble("y").toFloat())
    drawPath(path, color, style = Stroke(width = width, cap = StrokeCap.Round, join = StrokeJoin.Round))
}

private fun DrawScope.drawMedia(element: JSONObject, notebook: NotebookDoc) {
    val bounds = element.optJSONObject("bounds") ?: return
    val asset = notebook.asset(element.optString("asset_id")) ?: return
    val bytes = try {
        Base64.decode(asset.optString("data_base64"), Base64.DEFAULT)
    } catch (_: Exception) {
        return
    }
    val bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size) ?: return
    drawImage(
        bitmap.asImageBitmap(),
        dstOffset = androidx.compose.ui.unit.IntOffset(
            bounds.optDouble("x").toInt(),
            bounds.optDouble("y").toInt(),
        ),
        dstSize = androidx.compose.ui.unit.IntSize(
            max(1, bounds.optDouble("width").toInt()),
            max(1, bounds.optDouble("height").toInt()),
        ),
    )
}

private fun DrawScope.drawSpreadsheet(book: JSONObject) {
    val origin = book.optJSONObject("origin")
    val ox = origin?.optDouble("x", 48.0)?.toFloat() ?: 48f
    val oy = origin?.optDouble("y", 36.0)?.toFloat() ?: 36f
    val sheets = book.optJSONArray("sheets") ?: return
    if (sheets.length() == 0) return
    val sheet = sheets.getJSONObject(book.optInt("active_sheet", 0).coerceIn(0, sheets.length() - 1))
    val cols = NotebookDoc.displayCols(sheet)
    val rows = NotebookDoc.displayRows(sheet)
    val headerW = 36f
    val headerH = 22f
    val titleH = 26f
    val tabH = 22f
    val cellW = 64f
    val cellH = 21f
    val width = headerW + cols * cellW
    val height = titleH + headerH + rows * cellH + tabH
    drawRect(Color.White, Offset(ox, oy), Size(width, height))
    drawRect(Color(0xFF2E6B49), Offset(ox, oy), Size(width, titleH))
    val titlePaint = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply {
        color = android.graphics.Color.WHITE
        textSize = 12f
        isFakeBoldText = true
    }
    drawContext.canvas.nativeCanvas.drawText(
        "${sheet.optString("name")} · ${cols}×$rows",
        ox + 10f,
        oy + 17f,
        titlePaint,
    )
    val line = Color(0xC7C7D1C7)
    val header = Color(0xFFE8EDE8)
    val label = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply {
        color = 0xFF475047.toInt()
        textSize = 10f
    }
    val gridTop = oy + titleH
    drawRect(header, Offset(ox, gridTop), Size(width, headerH))
    for (col in 0 until cols) {
        val x = ox + headerW + col * cellW
        drawContext.canvas.nativeCanvas.drawText(NotebookDoc.colName(col), x + 6f, gridTop + 15f, label)
        drawLine(line, Offset(x, gridTop), Offset(x, oy + height - tabH), 1f)
    }
    for (row in 0 until rows) {
        val y = gridTop + headerH + row * cellH
        drawRect(header, Offset(ox, y), Size(headerW, cellH))
        drawContext.canvas.nativeCanvas.drawText("${row + 1}", ox + 8f, y + cellH * 0.7f, label)
        drawLine(line, Offset(ox, y), Offset(ox + width, y), 1f)
    }
    drawLine(line, Offset(ox + headerW, gridTop), Offset(ox + headerW, oy + height - tabH), 1f)
    val cells = sheet.optJSONObject("cells")
    if (cells != null) {
        val keys = cells.keys()
        val valuePaint = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply {
            color = 0xFF1A1F29.toInt()
            textSize = 11f
        }
        while (keys.hasNext()) {
            val addr = keys.next()
            val parsed = NotebookDoc.parseA1(addr) ?: continue
            val input = cells.optJSONObject(addr)?.optString("input").orEmpty()
            if (input.isBlank()) continue
            val x = ox + headerW + parsed.first * cellW + 4f
            val y = gridTop + headerH + parsed.second * cellH + cellH * 0.72f
            drawContext.canvas.nativeCanvas.drawText(input.take(10), x, y, valuePaint)
        }
    }
    drawRect(Color(0xFFF0F2F0), Offset(ox, oy + height - tabH), Size(width, tabH))
    drawRect(Color.White, Offset(ox + 8f, oy + height - tabH + 3f), Size(56f, 16f))
}
