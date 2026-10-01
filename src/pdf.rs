use crate::document::DocumentError;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn count_pdf_pages(bytes: &[u8]) -> usize {
    let text = String::from_utf8_lossy(bytes);
    let pages = text.matches("/Type /Page").count();
    let trees = text.matches("/Type /Pages").count();
    pages.saturating_sub(trees).max(1)
}

pub fn rasterize_pdf_pages(bytes: &[u8], dpi: u32) -> Result<Vec<Vec<u8>>, DocumentError> {
    let dir = std::env::temp_dir().join(format!("inkstone-pdf-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir)?;
    let pdf_path = dir.join("source.pdf");
    fs::write(&pdf_path, bytes)?;
    let dpi = dpi.clamp(72, 300).to_string();
    let pages = rasterize_with_pdftoppm(&pdf_path, &dir, &dpi)
        .or_else(|_| rasterize_with_ghostscript(&pdf_path, &dir, &dpi))
        .unwrap_or_default();
    let _ = fs::remove_dir_all(&dir);
    Ok(pages)
}

fn rasterize_with_pdftoppm(
    pdf: &Path,
    dir: &Path,
    dpi: &str,
) -> Result<Vec<Vec<u8>>, DocumentError> {
    let status = Command::new("pdftoppm")
        .args(["-png", "-r", dpi])
        .arg(pdf)
        .arg(dir.join("page"))
        .status()
        .map_err(|error| DocumentError::Export(error.to_string()))?;
    if !status.success() {
        return Err(DocumentError::Export(
            "pdftoppm could not rasterize the PDF".to_owned(),
        ));
    }
    collect_pngs(dir, "page")
}

fn rasterize_with_ghostscript(
    pdf: &Path,
    dir: &Path,
    dpi: &str,
) -> Result<Vec<Vec<u8>>, DocumentError> {
    let output = dir.join("page-%d.png");
    let dpi_arg = format!("-r{dpi}");
    let status = Command::new("gs")
        .args([
            "-dSAFER",
            "-dBATCH",
            "-dNOPAUSE",
            "-sDEVICE=png16m",
            &dpi_arg,
        ])
        .arg(format!("-sOutputFile={}", output.display()))
        .arg(pdf)
        .status()
        .map_err(|error| DocumentError::Export(error.to_string()))?;
    if !status.success() {
        return Err(DocumentError::Export(
            "Ghostscript could not rasterize the PDF".to_owned(),
        ));
    }
    collect_pngs(dir, "page")
}

fn collect_pngs(dir: &Path, stem: &str) -> Result<Vec<Vec<u8>>, DocumentError> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension().and_then(|ext| ext.to_str()) == Some("png")
                && path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(stem))
        })
        .collect();
    files.sort();
    let mut pages = Vec::new();
    for path in files {
        pages.push(fs::read(path)?);
    }
    if pages.is_empty() {
        Err(DocumentError::Export(
            "the PDF rasterizer wrote no page images".to_owned(),
        ))
    } else {
        Ok(pages)
    }
}

pub fn record_command() -> Option<(&'static str, Vec<&'static str>)> {
    for (bin, args) in [
        ("pw-record", vec!["--"]),
        ("parecord", vec!["--file-format=wav"]),
        ("arecord", vec!["-f", "cd", "-t", "wav"]),
    ] {
        if Command::new("which")
            .arg(bin)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Some((bin, args));
        }
    }
    None
}

pub fn recorder_available() -> bool {
    record_command().is_some()
}
