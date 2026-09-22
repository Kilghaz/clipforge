//! Enforces the dependency direction between workspace crates.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use cargo_metadata::{DependencyKind, MetadataCommand};

/// Which internal crates each crate may depend on (normal + build deps).
/// Dev-dependencies are not restricted.
const ALLOWED: &[(&str, &[&str])] = &[
    ("clipforge-core", &[]),
    ("clipforge-jobs", &[]),
    ("clipforge-platform", &[]),
    ("clipforge-i18n", &[]),
    ("clipforge-media", &["clipforge-core"]),
    (
        "clipforge-library",
        &[
            "clipforge-core",
            "clipforge-media",
            "clipforge-jobs",
            "clipforge-platform",
        ],
    ),
    ("clipforge-render", &["clipforge-core", "clipforge-media"]),
    (
        "clipforge-export",
        &[
            "clipforge-core",
            "clipforge-render",
            "clipforge-media",
            "clipforge-jobs",
        ],
    ),
    (
        "clipforge-app",
        &[
            "clipforge-core",
            "clipforge-library",
            "clipforge-render",
            "clipforge-export",
            "clipforge-jobs",
            "clipforge-platform",
            "clipforge-i18n",
            "clipforge-media",
        ],
    ),
    ("xtask", &[]),
];

/// Crates that must never appear in the dependency tree of a crate
/// (transitively), because they would drag UI or OS concerns into the core.
const FORBIDDEN_TRANSITIVE: &[(&str, &[&str])] = &[
    (
        "clipforge-core",
        &[
            "slint", "winit", "wgpu", "tokio", "rusqlite", "objc2", "windows",
        ],
    ),
    ("clipforge-render", &["slint", "winit", "rusqlite"]),
    ("clipforge-media", &["slint", "winit"]),
    ("clipforge-library", &["slint", "winit", "wgpu"]),
    ("clipforge-export", &["slint", "winit"]),
];

pub(crate) fn check() -> Result<()> {
    let metadata = MetadataCommand::new().exec().context("cargo metadata")?;
    let violations = find_violations(&metadata);
    if violations.is_empty() {
        println!("dependency direction OK");
        Ok(())
    } else {
        for v in &violations {
            eprintln!("{v}");
        }
        bail!("{} dependency rule violation(s)", violations.len());
    }
}

fn find_violations(metadata: &cargo_metadata::Metadata) -> Vec<String> {
    let allowed: BTreeMap<&str, &[&str]> = ALLOWED.iter().copied().collect();
    let mut violations = Vec::new();
    let workspace: Vec<_> = metadata.workspace_packages();

    for pkg in &workspace {
        let name = pkg.name.as_str();
        let Some(allowed_deps) = allowed.get(name) else {
            violations.push(format!("{name}: not listed in xtask/src/deps.rs ALLOWED; add it with its permitted dependencies"));
            continue;
        };
        for dep in &pkg.dependencies {
            let internal = workspace.iter().any(|p| p.name.as_str() == dep.name);
            if internal
                && dep.kind != DependencyKind::Development
                && !allowed_deps.contains(&dep.name.as_str())
            {
                violations.push(format!("{name} must not depend on {}", dep.name));
            }
        }
    }

    if let Some(resolve) = &metadata.resolve {
        let by_id: BTreeMap<_, _> = resolve.nodes.iter().map(|n| (&n.id, n)).collect();
        let pkg_by_id: BTreeMap<_, _> = metadata.packages.iter().map(|p| (&p.id, p)).collect();
        for (crate_name, forbidden) in FORBIDDEN_TRANSITIVE {
            let Some(root) = workspace.iter().find(|p| p.name.as_str() == *crate_name) else {
                continue;
            };
            let mut stack = vec![&root.id];
            let mut seen = std::collections::BTreeSet::new();
            while let Some(id) = stack.pop() {
                if !seen.insert(id) {
                    continue;
                }
                let Some(node) = by_id.get(id) else { continue };
                for dep in &node.deps {
                    // Only follow normal and build dependencies.
                    if dep
                        .dep_kinds
                        .iter()
                        .all(|k| k.kind == DependencyKind::Development)
                    {
                        continue;
                    }
                    if let Some(p) = pkg_by_id.get(&dep.pkg)
                        && forbidden.contains(&p.name.as_str())
                    {
                        violations.push(format!(
                            "{crate_name} transitively depends on forbidden crate {}",
                            p.name
                        ));
                    }
                    stack.push(&dep.pkg);
                }
            }
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_respects_dependency_direction() {
        let metadata = MetadataCommand::new().exec().unwrap();
        let violations = find_violations(&metadata);
        assert!(violations.is_empty(), "{}", violations.join("\n"));
    }

    #[test]
    fn every_workspace_crate_has_a_rule() {
        let metadata = MetadataCommand::new().exec().unwrap();
        for pkg in metadata.workspace_packages() {
            assert!(
                ALLOWED.iter().any(|(n, _)| *n == pkg.name.as_str()),
                "{} missing in ALLOWED",
                pkg.name
            );
        }
    }
}
