use crate::model::{is_image, ImageItem};
use crossbeam_channel::{bounded, Receiver};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use walkdir::WalkDir;

pub struct ScanOutcome {
    pub generation: u64,
    pub items: Vec<ImageItem>,
    pub elapsed: Duration,
    pub error: Option<String>,
}

pub fn spawn_scan(dir: PathBuf, recursive: bool, generation: u64) -> Receiver<ScanOutcome> {
    let (tx, rx) = bounded(1);
    std::thread::Builder::new()
        .name("gallery-scan".to_owned())
        .spawn(move || {
            let started = Instant::now();
            let result = if recursive {
                collect_recursive(&dir)
            } else {
                collect_flat(&dir)
            };

            let outcome = match result {
                Ok(mut items) => {
                    sort_by_name(&mut items);
                    ScanOutcome {
                        generation,
                        items,
                        elapsed: started.elapsed(),
                        error: None,
                    }
                }
                Err(error) => ScanOutcome {
                    generation,
                    items: Vec::new(),
                    elapsed: started.elapsed(),
                    error: Some(error),
                },
            };
            let _ = tx.send(outcome);
        })
        .ok();
    rx
}

fn collect_flat(dir: &Path) -> Result<Vec<ImageItem>, String> {
    let entries = std::fs::read_dir(dir).map_err(|err| format!("Cannot open folder: {err}"))?;
    let mut items = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_image(&path) {
            continue;
        }
        if let Some(item) = ImageItem::new(path) {
            items.push(item);
        }
    }
    Ok(items)
}

fn collect_recursive(dir: &Path) -> Result<Vec<ImageItem>, String> {
    let mut items = Vec::new();
    let walker = WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            !(entry.file_type().is_dir() && name.starts_with('.'))
        });

    for entry in walker.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !is_image(path) {
            continue;
        }
        if let Some(item) = ImageItem::new(path.to_path_buf()) {
            items.push(item);
        }
    }
    Ok(items)
}

fn sort_by_name(items: &mut [ImageItem]) {
    items.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });
}
