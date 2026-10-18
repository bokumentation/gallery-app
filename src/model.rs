use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "tif", "tiff"];

pub fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let ext = ext.to_ascii_lowercase();
            IMAGE_EXTENSIONS.contains(&ext.as_str())
        })
        .unwrap_or(false)
}

#[derive(Clone, Debug)]
pub struct ImageItem {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub modified: Option<SystemTime>,
    /// Filesystem birth time, when the platform reports it.
    pub created: Option<SystemTime>,
}

impl ImageItem {
    pub fn new(path: PathBuf) -> Option<Self> {
        let metadata = std::fs::metadata(&path).ok()?;
        if !metadata.is_file() {
            return None;
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Some(Self {
            path,
            name,
            size: metadata.len(),
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortKey {
    Name,
    Modified,
    Size,
}

impl SortKey {
    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Modified => "Date",
            Self::Size => "Size",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Name => Self::Modified,
            Self::Modified => Self::Size,
            Self::Size => Self::Name,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThumbSize {
    Small,
    Medium,
    Large,
}

impl ThumbSize {
    pub fn px(self) -> f32 {
        match self {
            Self::Small => 150.0,
            Self::Medium => 220.0,
            Self::Large => 310.0,
        }
    }
}
