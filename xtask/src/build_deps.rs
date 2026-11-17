//! Documentation check: the README's build prerequisites are exactly the list
//! in `docs/BUILD-DEPS.md`.
//!
//! Two places have to carry the package names — a reader starts at the README,
//! and the workflow installs the file it reads — so a divergence is a real
//! defect: the README would under-install for a judge, or the workflow would
//! install something the document does not justify. Both blocks sit between the
//! same marker pair, and only the package lines between them are compared, so
//! the surrounding prose in either file is free to differ.

/// Opening marker of a package list, in both documents.
const BEGIN: &str = "<!-- build-deps:begin -->";

/// Closing marker of a package list, in both documents.
const END: &str = "<!-- build-deps:end -->";

/// The package names a document lists between its markers, in order.
///
/// `None` when a marker is missing, they are out of order, or the block is
/// empty. Fence lines and blank lines are skipped so the block can be a fenced
/// list; a `#` line is a comment and is skipped too.
pub(crate) fn parse_package_list(text: &str) -> Option<Vec<String>> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let begin = lines.iter().position(|line| *line == BEGIN)?;
    let end = lines.iter().position(|line| *line == END)?;
    if end <= begin {
        return None;
    }
    let packages: Vec<String> = lines[begin + 1..end]
        .iter()
        .filter(|line| !line.is_empty() && !line.starts_with("```") && !line.starts_with('#'))
        .map(|line| (*line).to_string())
        .collect();
    if packages.is_empty() {
        None
    } else {
        Some(packages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn committed(relative: &str) -> String {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives in a subdirectory of the workspace");
        std::fs::read_to_string(root.join(relative))
            .unwrap_or_else(|error| panic!("failed to read {relative}: {error}"))
    }

    #[test]
    fn the_readme_prerequisites_are_the_documented_build_deps() {
        let readme = committed("README.md");
        let document = committed("docs/BUILD-DEPS.md");
        let from_readme = parse_package_list(&readme)
            .expect("README.md must carry a build-deps list between its markers");
        let from_document = parse_package_list(&document)
            .expect("docs/BUILD-DEPS.md must carry a build-deps list between its markers");
        assert_eq!(
            from_readme, from_document,
            "the README's prerequisites and docs/BUILD-DEPS.md disagree; change both lists together"
        );
        let mut sorted = from_document.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            from_document.len(),
            "docs/BUILD-DEPS.md lists a package twice: {from_document:?}"
        );
        for package in &from_document {
            assert!(
                package
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '+' | '.')),
                "`{package}` is not a package name"
            );
        }
    }

    #[test]
    fn a_missing_marker_is_not_a_list() {
        assert_eq!(parse_package_list("build-essential\n"), None);
        assert_eq!(parse_package_list(&format!("{BEGIN}\npkg-config\n")), None);
        assert_eq!(parse_package_list(&format!("{BEGIN}\n{END}\n")), None);
        assert_eq!(
            parse_package_list(&format!("{END}\npkg-config\n{BEGIN}\n")),
            None
        );
    }

    #[test]
    fn a_fenced_list_is_read_without_the_fence() {
        let text = format!("intro\n{BEGIN}\n```bash\npkg-config\nlibclang-dev\n```\n{END}\ntail\n");
        assert_eq!(
            parse_package_list(&text),
            Some(vec!["pkg-config".to_string(), "libclang-dev".to_string()])
        );
    }
}
