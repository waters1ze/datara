use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
fn test_phase5_docs_consistency_script() {
    let python_cmd = if Command::new("python3").arg("--version").output().is_ok() {
        "python3"
    } else {
        "python"
    };
    let output = Command::new(python_cmd)
        .arg("scripts/check_docs_consistency.py")
        .output()
        .expect("failed to execute check_docs_consistency.py");
    assert!(
        output.status.success(),
        "check_docs_consistency.py failed (exit: {:?})\nSTDOUT:\n{}\nSTDERR:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_phase5_glossary_and_changelog() {
    let glossary_path = Path::new("docs/GLOSSARY.md");
    assert!(glossary_path.exists(), "missing docs/GLOSSARY.md");
    let glossary = fs::read_to_string(glossary_path).expect("read glossary");
    assert!(
        glossary.contains("Affine Ownership"),
        "missing Affine Ownership in glossary"
    );
    assert!(
        glossary.contains("Capability Lattice"),
        "missing Capability Lattice in glossary"
    );
    assert!(
        glossary.contains("Evidence Gates"),
        "missing Evidence Gates in glossary"
    );

    let changelog_path = Path::new("CHANGELOG.md");
    assert!(changelog_path.exists(), "missing CHANGELOG.md");
    let changelog = fs::read_to_string(changelog_path).expect("read changelog");
    assert!(
        changelog.contains("## [1.1.0]"),
        "missing [1.1.0] in CHANGELOG.md"
    );
    assert!(changelog.contains("Native Async/Await to Completion"));
}

#[test]
fn test_phase5_readme_navigation() {
    let readme_path = Path::new("docs/README.md");
    assert!(readme_path.exists(), "missing docs/README.md");
    let readme = fs::read_to_string(readme_path).expect("read docs/README.md");
    assert!(
        readme.contains("TUTORIAL.md"),
        "missing TUTORIAL.md in docs/README.md"
    );
    assert!(
        readme.contains("GLOSSARY.md"),
        "missing GLOSSARY.md in docs/README.md"
    );
    assert!(
        readme.contains("Historical & Archival Specifications"),
        "missing archival specs section"
    );
}
