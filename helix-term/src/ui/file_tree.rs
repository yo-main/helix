use helix_view::{graphics::{Modifier, Rect}, Editor};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use tui::buffer::Buffer as Surface;

pub struct TreeEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_separator: bool,
    pub depth: usize,
    pub is_current: bool,
    pub is_modified: bool,
}

/// File tree showing open buffers grouped by their immediate parent directory.
pub struct FileTree {
    pub entries: Vec<TreeEntry>,
}

impl FileTree {
    pub fn build(
        root: &Path,
        current_file: Option<&Path>,
        open_files: &[&Path],
        modified_files: &HashSet<PathBuf>,
    ) -> Self {
        let root = match root.canonicalize() {
            Ok(p) => p,
            Err(_) => root.to_path_buf(),
        };

        let current_file_canonical = current_file.and_then(|p| p.canonicalize().ok());

        // Group files by their parent directory path relative to root.
        // BTreeMap keeps groups sorted alphabetically.
        // Values: (file_name, is_current, is_modified)
        let mut groups: BTreeMap<String, Vec<(String, bool, bool)>> = BTreeMap::new();

        for &file in open_files {
            let file_canonical = match file.canonicalize() {
                Ok(p) => p,
                Err(_) => continue,
            };

            let relative = match file_canonical.strip_prefix(&root) {
                Ok(rel) => rel.to_path_buf(),
                Err(_) => continue,
            };

            let is_current = current_file_canonical
                .as_ref()
                .map(|c| c == &file_canonical)
                .unwrap_or(false);

            let is_modified = modified_files.contains(&file_canonical);

            let file_name = relative
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            // Key is the parent path string, or empty for root-level files
            let parent_key = match relative.parent() {
                Some(p) if p.components().count() > 0 => p.to_string_lossy().to_string(),
                _ => String::new(),
            };

            groups
                .entry(parent_key)
                .or_default()
                .push((file_name, is_current, is_modified));
        }

        // Sort files within each group alphabetically
        for files in groups.values_mut() {
            files.sort_by(|a, b| a.0.cmp(&b.0));
        }

        let root_name = root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());

        let keys: Vec<&str> = groups.keys().map(|s| s.as_str()).collect();
        let dir_displays = compute_dir_displays(&keys, &root_name);

        let mut entries = Vec::new();
        for (dir_path, files) in &groups {
            let dir_display = dir_displays
                .get(dir_path.as_str())
                .cloned()
                .unwrap_or_else(|| dir_path.clone());

            // Split multi-component display paths into one line per component
            let components: Vec<&str> = dir_display.split('/').collect();
            for (i, component) in components.iter().enumerate() {
                entries.push(TreeEntry {
                    name: component.to_string(),
                    is_dir: true,
                    is_separator: false,
                    depth: i,
                    is_current: false,
                    is_modified: false,
                });
            }

            let file_depth = components.len();
            for (file_name, is_current, is_modified) in files {
                entries.push(TreeEntry {
                    name: file_name.clone(),
                    is_dir: false,
                    is_separator: false,
                    depth: file_depth,
                    is_current: *is_current,
                    is_modified: *is_modified,
                });
            }

            entries.push(TreeEntry {
                name: String::new(),
                is_dir: false,
                is_separator: true,
                depth: 0,
                is_current: false,
                is_modified: false,
            });
        }

        FileTree { entries }
    }

    /// Get list of file paths in tree display order (for navigation).
    /// Order matches build() exactly: groups sorted by full parent path, files sorted within groups.
    pub fn file_paths_in_order(root: &Path, open_files: &[&Path]) -> Vec<PathBuf> {
        let root = match root.canonicalize() {
            Ok(p) => p,
            Err(_) => root.to_path_buf(),
        };

        let mut groups: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();

        for &file in open_files {
            let file_canonical = match file.canonicalize() {
                Ok(p) => p,
                Err(_) => continue,
            };

            let relative = match file_canonical.strip_prefix(&root) {
                Ok(rel) => rel.to_path_buf(),
                Err(_) => continue,
            };

            let parent_key = match relative.parent() {
                Some(p) if p.components().count() > 0 => p.to_string_lossy().to_string(),
                _ => String::new(),
            };

            groups.entry(parent_key).or_default().push(file_canonical);
        }

        for files in groups.values_mut() {
            files.sort();
        }

        groups.into_values().flatten().collect()
    }
}

/// Compute display names for directory groups: leaf folder name when unambiguous,
/// expanding to more parent components only on collision.
fn compute_dir_displays<'a>(keys: &[&'a str], root_name: &str) -> HashMap<&'a str, String> {
    let mut depths: HashMap<&str, usize> = keys.iter().map(|&k| (k, 1usize)).collect();

    for _ in 0..20 {
        let mut display: HashMap<&str, String> = HashMap::new();
        for &key in keys {
            let d = if key.is_empty() {
                root_name.to_string()
            } else {
                let components: Vec<_> = Path::new(key).components().collect();
                let depth = depths[key].min(components.len());
                let relevant: PathBuf = components[components.len() - depth..].iter().collect();
                relevant.display().to_string()
            };
            display.insert(key, d);
        }

        let mut seen: HashMap<String, &str> = HashMap::new();
        let mut colliders: HashSet<&str> = HashSet::new();
        for &key in keys {
            let d = display[key].clone();
            if let Some(prev) = seen.insert(d.clone(), key) {
                colliders.insert(prev);
                colliders.insert(key);
            }
        }

        if colliders.is_empty() {
            return display;
        }

        for &key in &colliders {
            if !key.is_empty() {
                let max_depth = Path::new(key).components().count();
                let d = depths.entry(key).or_insert(1);
                if *d < max_depth {
                    *d += 1;
                }
            }
        }
    }

    // Fallback: return full paths
    keys.iter()
        .map(|&k| {
            let d = if k.is_empty() {
                root_name.to_string()
            } else {
                k.to_string()
            };
            (k, d)
        })
        .collect()
}

/// Truncate a string keeping the end visible, ellipsis at the start
fn truncate_start(name: &str, max_width: usize) -> String {
    let char_count = name.chars().count();
    if char_count <= max_width {
        return name.to_string();
    }
    let available = max_width.saturating_sub(1); // 1 for "…"
    let end: String = name
        .chars()
        .rev()
        .take(available)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("…{}", end)
}

/// Render the file tree panel
pub fn render(editor: &Editor, area: Rect, surface: &mut Surface) {
    let config = editor.config();

    if !config.file_tree.enable {
        return;
    }

    // Get current document's path from the focused view
    let current_file = editor
        .tree
        .views()
        .find(|(view, _)| view.id == editor.tree.focus)
        .and_then(|(view, _)| editor.document(view.doc))
        .and_then(|doc| doc.path());

    // Get all open document paths
    let open_files: Vec<&Path> = editor
        .documents()
        .filter_map(|doc| doc.path())
        .collect();

    // Collect modified file paths
    let modified_paths: HashSet<PathBuf> = editor
        .documents()
        .filter(|doc| doc.is_modified())
        .filter_map(|doc| doc.path().map(Path::to_path_buf))
        .collect();

    // Use workspace root or current working directory
    let root = helix_stdx::env::current_working_dir();

    let tree = FileTree::build(&root, current_file, &open_files, &modified_paths);

    // Styles
    let base_style = editor.theme.get("ui.background");
    let cursorline_style = editor.theme.get("ui.cursorline.primary");
    let text_style = editor.theme.get("ui.text");
    let dir_style = editor.theme.get("ui.text.directory");

    // Fill background
    surface.set_style(area, base_style);

    let separator_style = editor.theme.get("ui.virtual.indent-guide");

    for (i, entry) in tree.entries.iter().enumerate() {
        if i as u16 >= area.height {
            break;
        }

        let y = area.y + i as u16;

        if entry.is_separator {
            continue;
        }

        let display = if entry.is_dir {
            let indent_len = 1 + entry.depth * 2;
            // visual width: indent + 2 (📂) + 1 (space) = indent + 3
            let available = (area.width as usize).saturating_sub(indent_len + 3);
            format!("{}📂 {}", " ".repeat(indent_len), truncate_start(&entry.name, available))
        } else {
            let raw_name = if entry.is_modified {
                format!("{} [+]", entry.name)
            } else {
                entry.name.clone()
            };
            let indent_len = 1 + entry.depth * 2;
            // visual width: indent + 2 (🗎) + 1 (space) = indent + 3
            let available = (area.width as usize).saturating_sub(indent_len + 3);
            format!("{}🗎 {}", " ".repeat(indent_len), truncate_start(&raw_name, available))
        };

        let style = if entry.is_dir {
            text_style.patch(dir_style)
        } else if entry.is_current {
            text_style.patch(cursorline_style)
        } else {
            text_style
        };

        if entry.is_current {
            surface.set_style(Rect::new(area.x, y, area.width, 1), cursorline_style);
        }

        surface.set_stringn(area.x, y, &display, area.width as usize, style);
    }

    // Draw vertical separator on the right edge
    let separator_x = area.x + area.width;
    for y in area.y..area.y + area.height {
        surface.set_string(separator_x, y, "\u{2502}", separator_style);
    }
}
