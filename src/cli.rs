use crate::document::{Color, DocumentError, Element, ListStyle, Point, TextNote};
use crate::notebook::{Notebook, NotebookPage, Section};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub fn is_cli_invocation(args: &[String]) -> bool {
    if args.is_empty() {
        return false;
    }
    matches!(
        args[0].as_str(),
        "cli"
            | "bridge"
            | "new"
            | "create"
            | "add-page"
            | "import-markdown"
            | "import-dir"
            | "list-pages"
            | "export-markdown"
            | "batch"
            | "--cli"
            | "--help-cli"
            | "-h-cli"
    )
}

pub fn run(args: &[String]) -> u8 {
    if args.is_empty() {
        print_help();
        return 0;
    }

    let subcmd = args[0].as_str();
    let rest = &args[1..];

    match subcmd {
        "cli" | "bridge" => {
            if rest.is_empty() {
                print_help();
                0
            } else {
                run(rest)
            }
        }
        "help" | "--help" | "-h" | "--help-cli" | "-h-cli" => {
            print_help();
            0
        }
        "new" | "create" => cmd_new(rest),
        "add-page" => cmd_add_page(rest),
        "import-markdown" => cmd_import_markdown(rest),
        "import-dir" => cmd_import_dir(rest),
        "list-pages" => cmd_list_pages(rest),
        "export-markdown" => cmd_export_markdown(rest),
        "batch" => cmd_batch(rest),
        other => {
            eprintln!("Unknown CLI command: {other}. Use 'inkstone --help-cli' for usage.");
            1
        }
    }
}

fn print_help() {
    println!(
        "Inkstone CLI & Agent Bridge
===========================

Usage:
  inkstone <command> [arguments...]
  inkstone bridge <command> [arguments...]

Commands:
  new <path> [--title <title>] [--sections <s1,s2,...>]
      Create a new .inkstone notebook.

  add-page <notebook> --title <title> [--section <name>] [--text <text> | --file <file>]
      Add a note page to an existing notebook.

  import-markdown <notebook> <markdown_file> [--title <title>] [--section <section>]
      Import a Markdown file into the notebook as a structured page.

  import-dir <notebook> <directory> [--section <section>]
      Import all markdown files in a directory as pages in the notebook.

  list-pages <notebook> [--json]
      List all sections and pages in a notebook.

  export-markdown <notebook> [--out <directory>]
      Export notes from each page into Markdown files.

  batch <batch.json | ->
      Batch create/update notebook with full sections and pages from JSON.
"
    );
}

fn cmd_new(args: &[String]) -> u8 {
    if args.is_empty() {
        eprintln!("Error: path required. Usage: inkstone new <path> [--title <title>]");
        return 1;
    }
    let path = Path::new(&args[0]);
    let mut title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_owned();
    let mut sections = vec!["Notes".to_owned(), "Quick Notes".to_owned()];

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--title" if i + 1 < args.len() => {
                title = args[i + 1].clone();
                i += 2;
            }
            "--sections" if i + 1 < args.len() => {
                sections = args[i + 1]
                    .split(',')
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty())
                    .collect();
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }

    let mut notebook = Notebook::default();
    notebook.title = title;
    let colors = [
        Color::BLUE,
        Color::rgb(0.05, 0.56, 0.32),
        Color::rgb(0.95, 0.62, 0.05),
        Color::rgb(0.84, 0.16, 0.20),
        Color::rgb(0.48, 0.20, 0.78),
    ];
    notebook.sections = sections
        .into_iter()
        .enumerate()
        .map(|(idx, s)| Section::named(s, colors[idx % colors.len()]))
        .collect();
    if let Some(first) = notebook.sections.first() {
        notebook.active_section = first.id;
        for p in &mut notebook.pages {
            p.section_id = first.id;
        }
    }

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    match notebook.save(path) {
        Ok(_) => {
            println!("Created notebook at {}", path.display());
            0
        }
        Err(e) => {
            eprintln!("Failed to save notebook: {e}");
            1
        }
    }
}

fn cmd_add_page(args: &[String]) -> u8 {
    if args.is_empty() {
        eprintln!("Error: notebook path required.");
        return 1;
    }
    let path = Path::new(&args[0]);
    let mut notebook = match load_or_init_notebook(path) {
        Ok(nb) => nb,
        Err(e) => {
            eprintln!("Could not load notebook: {e}");
            return 1;
        }
    };

    let mut title = "Untitled Page".to_owned();
    let mut section_name = None;
    let mut text_content = None;
    let mut file_path = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--title" if i + 1 < args.len() => {
                title = args[i + 1].clone();
                i += 2;
            }
            "--section" if i + 1 < args.len() => {
                section_name = Some(args[i + 1].clone());
                i += 2;
            }
            "--text" if i + 1 < args.len() => {
                text_content = Some(args[i + 1].clone());
                i += 2;
            }
            "--file" if i + 1 < args.len() => {
                file_path = Some(args[i + 1].clone());
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }

    let section_id = resolve_or_create_section(&mut notebook, section_name.as_deref());
    let mut page = NotebookPage::named(&title);
    page.section_id = section_id;

    let content = if let Some(fp) = file_path {
        fs::read_to_string(fp).unwrap_or_default()
    } else {
        text_content.unwrap_or_default()
    };

    let elements = parse_markdown_to_elements(&content);
    if let Some(layer) = page.layers.first_mut() {
        layer.elements = elements;
    }

    append_page(&mut notebook, page);

    if let Err(e) = notebook.save(path) {
        eprintln!("Error saving notebook: {e}");
        return 1;
    }
    println!("Added page '{}' to {}", title, path.display());
    0
}

fn cmd_import_markdown(args: &[String]) -> u8 {
    if args.len() < 2 {
        eprintln!("Usage: inkstone import-markdown <notebook> <markdown_file> [--title <title>] [--section <section>]");
        return 1;
    }
    let notebook_path = Path::new(&args[0]);
    let md_path = Path::new(&args[1]);

    let mut notebook = match load_or_init_notebook(notebook_path) {
        Ok(nb) => nb,
        Err(e) => {
            eprintln!("Error opening notebook: {e}");
            return 1;
        }
    };

    let mut title = md_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported Page")
        .to_owned();
    let mut section_name = None;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--title" if i + 1 < args.len() => {
                title = args[i + 1].clone();
                i += 2;
            }
            "--section" if i + 1 < args.len() => {
                section_name = Some(args[i + 1].clone());
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }

    let content = match fs::read_to_string(md_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read markdown file: {e}");
            return 1;
        }
    };

    let section_id = resolve_or_create_section(&mut notebook, section_name.as_deref());
    let mut page = NotebookPage::named(&title);
    page.section_id = section_id;
    if let Some(layer) = page.layers.first_mut() {
        layer.elements = parse_markdown_to_elements(&content);
    }
    append_page(&mut notebook, page);

    if let Err(e) = notebook.save(notebook_path) {
        eprintln!("Error saving notebook: {e}");
        return 1;
    }
    println!("Imported '{}' into {}", md_path.display(), notebook_path.display());
    0
}

fn cmd_import_dir(args: &[String]) -> u8 {
    if args.len() < 2 {
        eprintln!("Usage: inkstone import-dir <notebook> <directory> [--section <section>]");
        return 1;
    }
    let notebook_path = Path::new(&args[0]);
    let dir_path = Path::new(&args[1]);

    let mut notebook = match load_or_init_notebook(notebook_path) {
        Ok(nb) => nb,
        Err(e) => {
            eprintln!("Error opening notebook: {e}");
            return 1;
        }
    };

    let mut section_name = None;
    if args.len() >= 4 && args[2] == "--section" {
        section_name = Some(args[3].clone());
    }

    let entries = match fs::read_dir(dir_path) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("Could not read directory {}: {e}", dir_path.display());
            return 1;
        }
    };

    let mut md_files: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() {
            if let Some(ext) = p.extension() {
                if ext == "md" || ext == "markdown" || ext == "txt" {
                    md_files.push(p);
                }
            }
        }
    }
    md_files.sort();

    let section_id = resolve_or_create_section(&mut notebook, section_name.as_deref());
    let mut imported = 0;

    for file in md_files {
        let title = file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Note")
            .to_owned();
        if let Ok(content) = fs::read_to_string(&file) {
            let mut page = NotebookPage::named(&title);
            page.section_id = section_id;
            if let Some(layer) = page.layers.first_mut() {
                layer.elements = parse_markdown_to_elements(&content);
            }
            append_page(&mut notebook, page);
            imported += 1;
        }
    }

    if let Err(e) = notebook.save(notebook_path) {
        eprintln!("Error saving notebook: {e}");
        return 1;
    }
    println!("Imported {} files from {} into {}", imported, dir_path.display(), notebook_path.display());
    0
}

fn cmd_list_pages(args: &[String]) -> u8 {
    if args.is_empty() {
        eprintln!("Usage: inkstone list-pages <notebook> [--json]");
        return 1;
    }
    let path = Path::new(&args[0]);
    let json_mode = args.iter().any(|a| a == "--json");

    let notebook = match Notebook::load(path) {
        Ok(nb) => nb,
        Err(e) => {
            eprintln!("Failed to load notebook: {e}");
            return 1;
        }
    };

    if json_mode {
        #[derive(Serialize)]
        struct PageInfo<'a> {
            id: String,
            title: &'a str,
            section: String,
            elements: usize,
        }
        #[derive(Serialize)]
        struct NotebookInfo<'a> {
            title: &'a str,
            pages: Vec<PageInfo<'a>>,
        }

        let pages: Vec<PageInfo> = notebook
            .pages
            .iter()
            .map(|p| {
                let sname = notebook
                    .section(p.section_id)
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| "Unknown".to_owned());
                let elem_count = p.layers.iter().map(|l| l.elements.len()).sum();
                PageInfo {
                    id: p.id.to_string(),
                    title: &p.title,
                    section: sname,
                    elements: elem_count,
                }
            })
            .collect();

        let info = NotebookInfo {
            title: &notebook.title,
            pages,
        };
        if let Ok(s) = serde_json::to_string_pretty(&info) {
            println!("{s}");
        }
        return 0;
    }

    println!("Notebook: {} ({} pages)", notebook.title, notebook.pages.len());
    for (idx, page) in notebook.pages.iter().enumerate() {
        let sname = notebook
            .section(page.section_id)
            .map(|s| s.name.as_str())
            .unwrap_or("No section");
        let count: usize = page.layers.iter().map(|l| l.elements.len()).sum();
        println!("  [{:>2}] {} | Section: {} | {} elements", idx + 1, page.title, sname, count);
    }
    0
}

fn cmd_export_markdown(args: &[String]) -> u8 {
    if args.is_empty() {
        eprintln!("Usage: inkstone export-markdown <notebook> [--out <dir>]");
        return 1;
    }
    let path = Path::new(&args[0]);
    let out_dir = if args.len() >= 3 && args[1] == "--out" {
        PathBuf::from(&args[2])
    } else {
        path.parent().unwrap_or(Path::new(".")).to_path_buf()
    };
    let _ = fs::create_dir_all(&out_dir);

    let notebook = match Notebook::load(path) {
        Ok(nb) => nb,
        Err(e) => {
            eprintln!("Failed to load notebook: {e}");
            return 1;
        }
    };

    for (idx, page) in notebook.pages.iter().enumerate() {
        let mut md = format!("# {}\n\n", page.title);
        for layer in &page.layers {
            for elem in &layer.elements {
                if let Element::Text(txt) = elem {
                    match txt.list {
                        ListStyle::Bullet => md.push_str(&format!("- {}\n", txt.text)),
                        ListStyle::Numbered => md.push_str(&format!("1. {}\n", txt.text)),
                        ListStyle::Checklist => {
                            let mark = if txt.checked { "x" } else { " " };
                            md.push_str(&format!("- [{}] {}\n", mark, txt.text));
                        }
                        ListStyle::None => {
                            if txt.bold && txt.font_size > 18.0 {
                                md.push_str(&format!("## {}\n\n", txt.text));
                            } else if txt.bold {
                                md.push_str(&format!("### {}\n\n", txt.text));
                            } else {
                                md.push_str(&format!("{}\n\n", txt.text));
                            }
                        }
                    }
                }
            }
        }
        let safe_title = page.title.replace('/', "_").replace('\\', "_");
        let filename = format!("{:02}_{}.md", idx + 1, safe_title);
        let dest = out_dir.join(filename);
        let _ = fs::write(&dest, md);
    }
    println!("Exported pages to {}", out_dir.display());
    0
}

#[derive(Deserialize)]
struct BatchSpec {
    path: String,
    title: Option<String>,
    #[serde(default)]
    sections: Vec<BatchSection>,
    #[serde(default)]
    pages: Vec<BatchPage>,
}

#[derive(Deserialize)]
struct BatchSection {
    name: String,
    #[serde(default)]
    color: Option<[f32; 3]>,
}

#[derive(Deserialize)]
struct BatchPage {
    title: String,
    #[serde(default)]
    section: Option<String>,
    #[serde(default)]
    markdown: Option<String>,
    #[serde(default)]
    blocks: Vec<BatchBlock>,
}

#[derive(Deserialize)]
struct BatchBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: String,
    #[serde(default)]
    level: Option<u32>,
    #[serde(default)]
    checked: Option<bool>,
}

fn cmd_batch(args: &[String]) -> u8 {
    if args.is_empty() {
        eprintln!("Usage: inkstone batch <spec.json | ->");
        return 1;
    }

    let json_str = if args[0] == "-" {
        use std::io::Read;
        let mut buf = String::new();
        if let Err(e) = std::io::stdin().read_to_string(&mut buf) {
            eprintln!("Failed to read from stdin: {e}");
            return 1;
        }
        buf
    } else {
        match fs::read_to_string(&args[0]) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to read {}: {e}", args[0]);
                return 1;
            }
        }
    };

    let spec: BatchSpec = match serde_json::from_str(&json_str) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to parse batch JSON: {e}");
            return 1;
        }
    };

    let path = Path::new(&spec.path);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let mut notebook = Notebook::default();
    if let Some(t) = spec.title {
        notebook.title = t;
    }

    let default_palette = [
        Color::BLUE,
        Color::rgb(0.05, 0.56, 0.32),
        Color::rgb(0.95, 0.62, 0.05),
        Color::rgb(0.84, 0.16, 0.20),
        Color::rgb(0.48, 0.20, 0.78),
    ];

    if !spec.sections.is_empty() {
        notebook.sections = spec
            .sections
            .iter()
            .enumerate()
            .map(|(idx, s)| {
                let col = if let Some([r, g, b]) = s.color {
                    Color::rgb(r, g, b)
                } else {
                    default_palette[idx % default_palette.len()]
                };
                Section::named(&s.name, col)
            })
            .collect();
        if let Some(first) = notebook.sections.first() {
            notebook.active_section = first.id;
        }
    }

    let mut first_page_used = false;
    for bp in spec.pages {
        let section_id = resolve_or_create_section(&mut notebook, bp.section.as_deref());
        let mut page = NotebookPage::named(&bp.title);
        page.section_id = section_id;

        let mut elements = Vec::new();
        if let Some(md) = bp.markdown {
            elements.extend(parse_markdown_to_elements(&md));
        }

        let mut y = if elements.is_empty() { 60.0 } else { 80.0 };
        for b in bp.blocks {
            let elem = match b.block_type.as_str() {
                "h1" | "heading1" => {
                    y += 20.0;
                    let mut note = TextNote::plain(Point::new(60.0, y), b.text, 24.0, Color::INK);
                    note.bold = true;
                    note.max_width = Some(800.0);
                    y += 40.0;
                    Element::Text(note)
                }
                "h2" | "heading2" => {
                    y += 15.0;
                    let mut note = TextNote::plain(Point::new(60.0, y), b.text, 18.0, Color::BLUE);
                    note.bold = true;
                    note.max_width = Some(800.0);
                    y += 32.0;
                    Element::Text(note)
                }
                "h3" | "heading3" => {
                    y += 10.0;
                    let mut note = TextNote::plain(Point::new(60.0, y), b.text, 15.0, Color::INK);
                    note.bold = true;
                    note.max_width = Some(800.0);
                    y += 26.0;
                    Element::Text(note)
                }
                "bullet" => {
                    let lines = b.text.lines().count().max(1);
                    let mut note = TextNote::plain(Point::new(60.0, y), b.text, 14.0, Color::INK);
                    note.list = ListStyle::Bullet;
                    note.max_width = Some(800.0);
                    y += (lines as f32) * 20.0 + 8.0;
                    Element::Text(note)
                }
                "todo" => {
                    let mut note = TextNote::plain(Point::new(60.0, y), b.text, 14.0, Color::INK);
                    note.list = ListStyle::Checklist;
                    note.checked = b.checked.unwrap_or(false);
                    note.max_width = Some(800.0);
                    y += 25.0;
                    Element::Text(note)
                }
                _ => {
                    let lines = b.text.lines().count().max(1);
                    let mut note = TextNote::plain(Point::new(60.0, y), b.text, 14.0, Color::INK);
                    note.max_width = Some(800.0);
                    y += (lines as f32) * 20.0 + 12.0;
                    Element::Text(note)
                }
            };
            elements.push(elem);
        }

        if let Some(layer) = page.layers.first_mut() {
            layer.elements = elements;
        }

        if !first_page_used && notebook.pages.len() == 1 && notebook.pages[0].elements().count() == 0 {
            page.id = notebook.pages[0].id;
            notebook.pages[0] = page;
            first_page_used = true;
        } else {
            notebook.pages.push(page);
        }
    }

    if let Err(e) = notebook.save(path) {
        eprintln!("Error saving batch notebook {}: {e}", path.display());
        return 1;
    }

    println!("Batch successfully created notebook at {}", path.display());
    0
}

fn load_or_init_notebook(path: &Path) -> Result<Notebook, DocumentError> {
    if path.exists() {
        Notebook::load(path)
    } else {
        let mut nb = Notebook::default();
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Notebook");
        nb.title = stem.to_owned();
        Ok(nb)
    }
}

fn resolve_or_create_section(notebook: &mut Notebook, section_name: Option<&str>) -> Uuid {
    if let Some(name) = section_name {
        if let Some(sec) = notebook.sections.iter().find(|s| s.name.eq_ignore_ascii_case(name)) {
            return sec.id;
        }
        let colors = [
            Color::BLUE,
            Color::rgb(0.05, 0.56, 0.32),
            Color::rgb(0.95, 0.62, 0.05),
            Color::rgb(0.84, 0.16, 0.20),
            Color::rgb(0.48, 0.20, 0.78),
        ];
        let color = colors[notebook.sections.len() % colors.len()];
        let new_sec = Section::named(name, color);
        let id = new_sec.id;
        notebook.sections.push(new_sec);
        id
    } else {
        notebook.active_section
    }
}

fn append_page(notebook: &mut Notebook, page: NotebookPage) {
    if notebook.pages.len() == 1 && notebook.pages[0].elements().count() == 0 {
        notebook.pages[0] = page;
    } else {
        notebook.pages.push(page);
    }
}

fn parse_markdown_to_elements(content: &str) -> Vec<Element> {
    let mut elements = Vec::new();
    let mut y = 60.0;

    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() {
            y += 10.0;
            i += 1;
            continue;
        }

        if let Some(stripped) = line.strip_prefix("# ") {
            let mut note = TextNote::plain(Point::new(60.0, y), stripped.trim(), 24.0, Color::INK);
            note.bold = true;
            note.max_width = Some(800.0);
            y += 44.0;
            elements.push(Element::Text(note));
            i += 1;
            continue;
        }

        if let Some(stripped) = line.strip_prefix("## ") {
            let mut note = TextNote::plain(Point::new(60.0, y), stripped.trim(), 18.0, Color::BLUE);
            note.bold = true;
            note.max_width = Some(800.0);
            y += 34.0;
            elements.push(Element::Text(note));
            i += 1;
            continue;
        }

        if let Some(stripped) = line.strip_prefix("### ") {
            let mut note = TextNote::plain(Point::new(60.0, y), stripped.trim(), 15.0, Color::INK);
            note.bold = true;
            note.max_width = Some(800.0);
            y += 28.0;
            elements.push(Element::Text(note));
            i += 1;
            continue;
        }

        if let Some(stripped) = line.strip_prefix("- [ ] ") {
            let mut note = TextNote::plain(Point::new(60.0, y), stripped.trim(), 14.0, Color::INK);
            note.list = ListStyle::Checklist;
            note.checked = false;
            note.max_width = Some(800.0);
            y += 26.0;
            elements.push(Element::Text(note));
            i += 1;
            continue;
        }

        if let Some(stripped) = line.strip_prefix("- [x] ") {
            let mut note = TextNote::plain(Point::new(60.0, y), stripped.trim(), 14.0, Color::INK);
            note.list = ListStyle::Checklist;
            note.checked = true;
            note.max_width = Some(800.0);
            y += 26.0;
            elements.push(Element::Text(note));
            i += 1;
            continue;
        }

        if let Some(stripped) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            let mut note = TextNote::plain(Point::new(60.0, y), stripped.trim(), 14.0, Color::INK);
            note.list = ListStyle::Bullet;
            note.max_width = Some(800.0);
            let line_count = stripped.len() / 90 + 1;
            y += (line_count as f32) * 20.0 + 6.0;
            elements.push(Element::Text(note));
            i += 1;
            continue;
        }

        // Paragraph - collect consecutive non-empty non-special lines
        let mut para_lines = Vec::new();
        while i < lines.len() {
            let current = lines[i].trim();
            if current.is_empty()
                || current.starts_with('#')
                || current.starts_with("- ")
                || current.starts_with("* ")
            {
                break;
            }
            para_lines.push(current);
            i += 1;
        }

        if !para_lines.is_empty() {
            let text = para_lines.join(" ");
            let line_count = (text.len() / 85).max(para_lines.len()).max(1);
            let mut note = TextNote::plain(Point::new(60.0, y), text, 14.0, Color::INK);
            note.max_width = Some(800.0);
            y += (line_count as f32) * 20.0 + 14.0;
            elements.push(Element::Text(note));
        }
    }

    elements
}
