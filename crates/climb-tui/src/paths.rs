//! Path rendering for the terminal: never draw an absolute path. Every surface that names a file
//! goes through [`display`], which prefers a path relative to the current
//! checkout, then a path starting at a recognizable repository directory, and
//! finally a short suffix that still identifies the file.

use std::path::{Component, Path};

/// Directories that mark the start of a repository-relative path.
const REPO_MARKERS: [&str; 5] = ["target", "docs", "crates", "xtask", "third_party"];

/// Render `path` without an absolute prefix.
///
/// Relative paths are returned as written (normalized only to `/` separators
/// through `Path::display`). An absolute path under the current directory is
/// made relative to it. Failing that, the path is cut at the last recognizable
/// repository directory (`target/`, `docs/`, …). A path that matches none of
/// those keeps only its final components, with a leading ellipsis.
pub fn display(path: &Path) -> String {
    if path.as_os_str().is_empty() {
        return String::new();
    }
    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(relative) = path.strip_prefix(&cwd) {
            return render(relative);
        }
    }
    if path.is_relative() {
        return render(path);
    }
    if let Some(relative) = repository_relative(path) {
        return relative;
    }
    suffix(path)
}

/// Join a relative path's normal components, so the display never starts with
/// a separator even on a path such as `/`.
fn render(path: &Path) -> String {
    let parts: Vec<String> = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            Component::ParentDir => Some("..".to_string()),
            _ => None,
        })
        .collect();
    parts.join("/")
}

/// Cut an absolute path at the last repository marker directory.
fn repository_relative(path: &Path) -> Option<String> {
    let components: Vec<Component> = path.components().collect();
    let mut start = None;
    for (index, component) in components.iter().enumerate() {
        if let Component::Normal(name) = component {
            let name = name.to_string_lossy();
            if REPO_MARKERS
                .iter()
                .any(|marker| name.eq_ignore_ascii_case(marker))
            {
                start = Some(index);
            }
        }
    }
    let start = start?;
    let parts: Vec<String> = components
        .get(start..)
        .unwrap_or(&[])
        .iter()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    Some(parts.join("/"))
}

/// Keep the final components of a path that is neither under the checkout nor
/// under a recognizable repository directory. Home-directory roots are dropped
/// first so the user's name never appears.
fn suffix(path: &Path) -> String {
    let mut parts: Vec<String> = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    if parts
        .first()
        .is_some_and(|first| matches!(first.as_str(), "home" | "Users" | "root"))
    {
        parts.drain(0..2.min(parts.len()));
    }
    let take = parts.len().min(3);
    let tail = parts.get(parts.len().saturating_sub(take)..).unwrap_or(&[]);
    format!("\u{2026}/{}", tail.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_are_returned_as_written() {
        assert_eq!(
            display(Path::new("target/climb-results.json")),
            "target/climb-results.json"
        );
        assert_eq!(display(Path::new("docs/BASELINES.md")), "docs/BASELINES.md");
    }

    #[test]
    fn an_absolute_checkout_path_is_cut_at_the_repository_directory() {
        let path = Path::new("/home/someone/work/climb/target/climb-results.json"); // path-hygiene:allow
        assert_eq!(display(path), "target/climb-results.json");
        let docs = Path::new("/Users/someone/climb/docs/BASELINES.md"); // path-hygiene:allow
        assert_eq!(display(docs), "docs/BASELINES.md");
    }

    #[test]
    fn a_path_outside_the_repository_keeps_only_a_short_suffix() {
        let rendered = display(Path::new("/home/someone/private/run/climb-results.json")); // path-hygiene:allow
        assert!(!rendered.contains("/home/"), "{rendered}");
        assert!(!rendered.contains("someone"), "{rendered}");
        assert!(rendered.ends_with("climb-results.json"), "{rendered}");
        let windows_like = display(Path::new("/Users/someone/run/climb-results.json")); // path-hygiene:allow
        assert!(!windows_like.contains("/Users/"), "{windows_like}");
        assert!(!windows_like.contains("someone"), "{windows_like}");
    }

    #[test]
    fn an_empty_path_stays_empty() {
        // path-hygiene:allow
        assert_eq!(display(Path::new("")), "");
    }
}
