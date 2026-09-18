use inkstone::document::{Color, Point};
use inkstone::notebook::{Layer, Notebook, NotebookPage};
use inkstone::spreadsheet::address::{CellAddr, CellRange, col_name, parse_col};
use inkstone::spreadsheet::formula::{ErrorKind, Value, adjust_formula, parse_literal};
use inkstone::spreadsheet::{Cell, CellStyle, HAlign, LayerKind, Sheet, Spreadsheet};
use tempfile::tempdir;

fn book_with(cells: &[(&str, &str)]) -> Spreadsheet {
    let mut book = Spreadsheet::new();
    for (addr, input) in cells {
        let addr = CellAddr::parse_a1(addr).unwrap();
        book.active_mut().set_input(addr, (*input).to_owned());
    }
    book
}

fn eval(cells: &[(&str, &str)], addr: &str) -> Value {
    let book = book_with(cells);
    book.evaluate(0, CellAddr::parse_a1(addr).unwrap())
}

fn num(cells: &[(&str, &str)], addr: &str) -> f64 {
    match eval(cells, addr) {
        Value::Number(value) => value,
        other => panic!("expected number at {addr}, got {other:?}"),
    }
}

#[test]
fn excel_arithmetic_precedence_and_percent() {
    assert_eq!(num(&[("A1", "=1+2*3")], "A1"), 7.0);
    assert_eq!(num(&[("A1", "=2^3^2")], "A1"), 512.0);
    assert_eq!(num(&[("A1", "=10%")], "A1"), 0.1);
    assert_eq!(num(&[("A1", "=-3+5")], "A1"), 2.0);
    assert_eq!(num(&[("A1", "=(1+2)*3")], "A1"), 9.0);
    assert!((num(&[("A1", "=2^10")], "A1") - 1024.0).abs() < f64::EPSILON);
}

#[test]
fn excel_errors_and_comparisons() {
    assert_eq!(eval(&[("A1", "=1/0")], "A1"), Value::Error(ErrorKind::Div0));
    assert_eq!(
        eval(&[("A1", "=1+\"x\"")], "A1"),
        Value::Error(ErrorKind::Value)
    );
    assert_eq!(
        eval(&[("A1", "=UNKNOWN()")], "A1"),
        Value::Error(ErrorKind::Name)
    );
    assert_eq!(eval(&[("A1", "=1=1")], "A1"), Value::Bool(true));
    assert_eq!(eval(&[("A1", "=1<>2")], "A1"), Value::Bool(true));
    assert_eq!(
        eval(&[("A1", "=\"a\"&\"b\"")], "A1"),
        Value::Text("ab".into())
    );
    assert_eq!(eval(&[("A1", "=TRUE+TRUE")], "A1"), Value::Number(2.0));
}

#[test]
fn vlookup_index_and_text_functions() {
    let cells = [
        ("A1", "east"),
        ("B1", "10"),
        ("A2", "west"),
        ("B2", "20"),
        ("A3", "east"),
        ("B3", "30"),
        ("C1", "=VLOOKUP(\"west\",A1:B3,2,FALSE)"),
        ("C2", "=INDEX(B1:B3,2)"),
        ("C3", "=MATCH(20,B1:B3,0)"),
        ("C4", "=SUMIF(A1:A3,\"east\",B1:B3)"),
        ("C5", "=LEFT(A1,2)&UPPER(RIGHT(A2,2))"),
        ("C6", "=IFERROR(1/0,42)"),
        ("C7", "=ROUND(10.46,1)"),
        ("C8", "=MAX(B1:B3)-MIN(B1:B3)"),
        ("C9", "=LEN(TRIM(\"  hi  \"))"),
        ("C10", "=CHOOSE(2,\"a\",\"b\",\"c\")"),
        ("C11", "=ROW(A3)"),
        ("C12", "=COLUMN(C1)"),
        ("C13", "=INDIRECT(\"B2\")"),
        ("C14", "=POWER(2,10)"),
        ("C15", "=ABS(-4)"),
        ("C16", "=AND(TRUE,1)"),
        ("C17", "=NOT(FALSE)"),
        ("C18", "=DATE(2024,1,1)"),
        ("C19", "=YEAR(DATE(2024,2,29))"),
        ("C20", "=MONTH(DATE(2024,2,29))"),
        ("C21", "=DAY(DATE(2024,2,29))"),
        ("C22", "=MEDIAN(1,5,3,9,2)"),
        ("C23", "=LARGE(B1:B3,1)"),
        ("C24", "=SMALL(B1:B3,1)"),
        ("C25", "=N(TRUE)"),
        ("C26", "=T(A1)"),
        ("C27", "=ISNUMBER(B1)"),
        ("C28", "=ISBLANK(Z99)"),
        ("C29", "=COUNT(B1:B3)"),
        ("C30", "=COUNTA(A1:A3)"),
        ("C31", "=PRODUCT(2,3,4)"),
        ("C32", "=MOD(10,3)"),
        ("C33", "=INT(3.9)"),
        ("C34", "=SQRT(81)"),
        ("C35", "=CONCAT(A1,\"-\",B1)"),
        ("C36", "=IF(B1>15,1,0)"),
        ("C37", "=ADDRESS(2,3)"),
        ("C38", "=ROWS(A1:B4)"),
        ("C39", "=COLUMNS(A1:D1)"),
        ("C40", "=OFFSET(B1,1,0)"),
    ];
    assert_eq!(num(&cells, "C1"), 20.0);
    assert_eq!(num(&cells, "C2"), 20.0);
    assert_eq!(num(&cells, "C3"), 2.0);
    assert_eq!(num(&cells, "C4"), 40.0);
    assert_eq!(eval(&cells, "C5"), Value::Text("eaST".into()));
    assert_eq!(num(&cells, "C6"), 42.0);
    assert_eq!(num(&cells, "C7"), 10.5);
    assert_eq!(num(&cells, "C8"), 20.0);
    assert_eq!(num(&cells, "C9"), 2.0);
    assert_eq!(eval(&cells, "C10"), Value::Text("b".into()));
    assert_eq!(num(&cells, "C11"), 3.0);
    assert_eq!(num(&cells, "C12"), 3.0);
    assert_eq!(num(&cells, "C13"), 20.0);
    assert_eq!(num(&cells, "C14"), 1024.0);
    assert_eq!(num(&cells, "C15"), 4.0);
    assert_eq!(eval(&cells, "C16"), Value::Bool(true));
    assert_eq!(eval(&cells, "C17"), Value::Bool(true));
    assert!(matches!(eval(&cells, "C18"), Value::Number(_)));
    assert_eq!(num(&cells, "C19"), 2024.0);
    assert_eq!(num(&cells, "C20"), 2.0);
    assert_eq!(num(&cells, "C21"), 29.0);
    assert_eq!(num(&cells, "C22"), 3.0);
    assert_eq!(num(&cells, "C23"), 30.0);
    assert_eq!(num(&cells, "C24"), 10.0);
    assert_eq!(num(&cells, "C25"), 1.0);
    assert_eq!(eval(&cells, "C26"), Value::Text("east".into()));
    assert_eq!(eval(&cells, "C27"), Value::Bool(true));
    assert_eq!(eval(&cells, "C28"), Value::Bool(true));
    assert_eq!(num(&cells, "C29"), 3.0);
    assert_eq!(num(&cells, "C30"), 3.0);
    assert_eq!(num(&cells, "C31"), 24.0);
    assert_eq!(num(&cells, "C32"), 1.0);
    assert_eq!(num(&cells, "C33"), 3.0);
    assert_eq!(num(&cells, "C34"), 9.0);
    assert_eq!(eval(&cells, "C35"), Value::Text("east-10".into()));
    assert_eq!(num(&cells, "C36"), 0.0);
    assert_eq!(eval(&cells, "C37"), Value::Text("$C$2".into()));
    assert_eq!(num(&cells, "C38"), 4.0);
    assert_eq!(num(&cells, "C39"), 4.0);
    assert_eq!(num(&cells, "C40"), 20.0);
}

#[test]
fn relative_and_absolute_copy_rewrites_a1_refs() {
    assert_eq!(adjust_formula("=A1+1", 1, 2), "=B3+1");
    assert_eq!(adjust_formula("=$A$1+B1", 2, 1), "=$A$1+D2");
    assert_eq!(adjust_formula("=SUM(A1:A3)", 1, 0), "=SUM(B1:B3)");
    assert_eq!(CellRange::parse("B2:D10").unwrap().a1(), "B2:D10");
    assert_eq!(CellRange::parse(" $c$3 ").unwrap().a1(), "C3");
}

#[test]
fn circular_references_are_detected() {
    let value = eval(&[("A1", "=B1"), ("B1", "=A1")], "A1");
    assert_eq!(value, Value::Error(ErrorKind::Circ));
}

#[test]
fn cross_sheet_references_resolve() {
    let mut book = Spreadsheet::new();
    book.sheets.push(Sheet::named("Costs"));
    book.sheets[1].set_input(CellAddr::parse_a1("A1").unwrap(), "5".into());
    book.sheets[0].set_input(CellAddr::parse_a1("B1").unwrap(), "=Costs!A1*2".into());
    assert_eq!(
        book.evaluate(0, CellAddr::parse_a1("B1").unwrap()),
        Value::Number(10.0)
    );
}

#[test]
fn quoted_sheet_names_and_string_escapes() {
    let mut book = Spreadsheet::new();
    book.sheets[0].name = "My Sheet".into();
    book.add_sheet();
    book.sheets[0].set_input(CellAddr::parse_a1("A1").unwrap(), "7".into());
    book.sheets[1].set_input(CellAddr::parse_a1("A1").unwrap(), "='My Sheet'!A1".into());
    book.sheets[1].set_input(
        CellAddr::parse_a1("B1").unwrap(),
        "=\"hello\"\"world\"".into(),
    );
    assert_eq!(
        book.evaluate(1, CellAddr::parse_a1("A1").unwrap()),
        Value::Number(7.0)
    );
    assert_eq!(
        book.evaluate(1, CellAddr::parse_a1("B1").unwrap()),
        Value::Text("hello\"world".into())
    );
}

#[test]
fn fill_and_sort_behave_like_a_worksheet() {
    let mut sheet = Sheet::named("Sheet1");
    sheet.set_input(CellAddr::parse_a1("A1").unwrap(), "1".into());
    sheet.set_input(CellAddr::parse_a1("A2").unwrap(), "2".into());
    sheet.fill(
        CellRange::parse("A1:A2").unwrap(),
        CellRange::parse("A1:A4").unwrap(),
    );
    assert_eq!(sheet.cells[&CellAddr::parse_a1("A3").unwrap()].input, "3");
    assert_eq!(sheet.cells[&CellAddr::parse_a1("A4").unwrap()].input, "4");

    sheet.set_input(CellAddr::parse_a1("B1").unwrap(), "c".into());
    sheet.set_input(CellAddr::parse_a1("B2").unwrap(), "a".into());
    sheet.set_input(CellAddr::parse_a1("B3").unwrap(), "b".into());
    sheet.sort_range(CellRange::parse("B1:B3").unwrap(), 1, true);
    assert_eq!(sheet.cells[&CellAddr::parse_a1("B1").unwrap()].input, "a");
    assert_eq!(sheet.cells[&CellAddr::parse_a1("B2").unwrap()].input, "b");
    assert_eq!(sheet.cells[&CellAddr::parse_a1("B3").unwrap()].input, "c");
}

#[test]
fn number_formats_and_styles_round_trip_in_notebook() {
    let mut notebook = Notebook::default();
    let mut layer = Layer::spreadsheet("Budget");
    {
        let book = layer.spreadsheet.as_mut().unwrap();
        let addr = CellAddr::parse_a1("A1").unwrap();
        book.active_mut().cells.insert(
            addr,
            Cell {
                input: "1234.5".into(),
                style: CellStyle {
                    bold: true,
                    number_format: "#,##0.00".into(),
                    fill: Some(Color::rgb(0.9, 0.95, 1.0)),
                    h_align: HAlign::Right,
                    ..CellStyle::default()
                },
            },
        );
        book.active_mut()
            .set_input(CellAddr::parse_a1("B1").unwrap(), "=A1*2".into());
    }
    notebook.pages[0].layers.push(layer);

    let directory = tempdir().unwrap();
    let path = directory.path().join("budget.inkstone");
    notebook.save(&path).unwrap();
    let loaded = Notebook::load(&path).unwrap();
    assert_eq!(loaded, notebook);
    assert_eq!(loaded.pages[0].layers[1].kind, LayerKind::Excel);
    let hits = loaded.search("1234");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].cell.as_deref(), Some("A1"));

    let svg_path = directory.path().join("budget.svg");
    loaded.export_svg(&svg_path, 0).unwrap();
    let svg = std::fs::read_to_string(svg_path).unwrap();
    assert!(svg.contains("data-kind=\"spreadsheet\""));
    assert!(svg.contains("1,234.50") || svg.contains("1234"));
}

#[test]
fn notes_documents_without_kind_still_load() {
    let json = r#"{
      "format": "inkstone.notebook",
      "version": 2,
      "title": "Legacy notes",
      "pages": [{
        "id": "d3f6e3d4-249c-4e75-9634-fef6404fca55",
        "title": "Page 1",
        "canvas": {
          "background": {"red": 0.98, "green": 0.98, "blue": 0.97, "alpha": 1.0},
          "grid_spacing": 24.0,
          "grid_visible": true
        },
        "layers": [{
          "id": "3de3a79c-b0ab-4b26-93aa-40fcfc4ad03a",
          "name": "Notes",
          "visible": true,
          "locked": false,
          "elements": []
        }]
      }],
      "assets": []
    }"#;
    let notebook: Notebook = serde_json::from_str(json).unwrap();
    notebook.validate().unwrap();
    assert_eq!(notebook.pages[0].layers[0].kind, LayerKind::Notes);
}

#[test]
fn spreadsheet_without_workbook_is_rejected() {
    let mut notebook = Notebook::default();
    notebook.pages[0].layers[0].kind = LayerKind::Excel;
    notebook.pages[0].layers[0].spreadsheet = None;
    assert!(
        notebook
            .validate()
            .unwrap_err()
            .to_string()
            .contains("missing workbook")
    );
}

#[test]
fn column_names_and_hit_testing_cover_the_visible_grid() {
    assert_eq!(col_name(0), "A");
    assert_eq!(parse_col("AA"), Some(26));
    let book = Spreadsheet {
        origin: Point::new(0.0, 0.0),
        ..Spreadsheet::new()
    };
    let first = book.cell_rect(CellAddr { col: 0, row: 0 });
    let hit = book
        .hit_cell(Point::new(first.x + 2.0, first.y + 2.0))
        .unwrap();
    assert_eq!(hit.a1(), "A1");
    assert!(book.hit_title(Point::new(10.0, 10.0)));
}

#[test]
fn parse_literal_distinguishes_numbers_and_text() {
    assert_eq!(parse_literal("42"), Value::Number(42.0));
    assert_eq!(parse_literal("10%"), Value::Number(0.1));
    assert_eq!(parse_literal("TRUE"), Value::Bool(true));
    assert_eq!(parse_literal("hello"), Value::Text("hello".into()));
}

#[test]
fn page_object_count_includes_spreadsheet_cells_in_search() {
    let mut page = NotebookPage::named("Calc");
    page.layers = vec![Layer::spreadsheet("Grid")];
    page.layers[0]
        .spreadsheet
        .as_mut()
        .unwrap()
        .active_mut()
        .set_input(CellAddr::parse_a1("A1").unwrap(), "motor winding".into());
    let mut notebook = Notebook::default();
    notebook.pages[0] = page;
    assert_eq!(notebook.search("winding").len(), 1);
}
