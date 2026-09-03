use super::manifest::DataraManifest;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectKind {
    SingleFile(PathBuf),      // Level 1: hello.dtr
    Directory(PathBuf),       // Level 2: myapp/ (zero manifest, convention-based)
    ManifestProject(PathBuf), // Level 3: full project with datara.toml
}

#[derive(Debug, Clone)]
pub struct ProjectLayout {
    pub root: PathBuf,
    pub kind: ProjectKind,
    pub manifest: Option<DataraManifest>,
    pub name: String,
    pub entry_point: PathBuf,
    pub source_files: Vec<PathBuf>,
    pub test_files: Vec<PathBuf>,
    pub example_files: Vec<PathBuf>,
    pub bench_files: Vec<PathBuf>,
}

impl ProjectLayout {
    /// Returns the target binary name derived from manifest target, package name, or entry file stem
    pub fn binary_name(&self) -> String {
        if let Some(ref m) = self.manifest {
            if let Some(ref t) = m.target
                && let Some(ref bin) = t.bin_name
            {
                return bin.clone();
            }
            if !m.package.name.is_empty() {
                return m.package.name.clone();
            }
        }
        self.entry_point
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("app")
            .to_string()
    }
}

pub struct ProjectDiscovery;

impl ProjectDiscovery {
    /// Discovers project layout from target path or current directory
    pub fn discover(target: Option<&Path>) -> Result<ProjectLayout, String> {
        let base = target.unwrap_or_else(|| Path::new("."));

        // Case 1: Target is an explicit single .dtr file (Level 1: Single File)
        if base.is_file() {
            if base.extension().and_then(|s| s.to_str()) == Some("dtr") {
                let name = base
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("app")
                    .to_string();
                let abs_path = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
                let root = abs_path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf();
                return Ok(ProjectLayout {
                    root,
                    kind: ProjectKind::SingleFile(abs_path.clone()),
                    manifest: None,
                    name,
                    entry_point: abs_path.clone(),
                    source_files: vec![abs_path],
                    test_files: Vec::new(),
                    example_files: Vec::new(),
                    bench_files: Vec::new(),
                });
            } else {
                return Err(format!(
                    "Target file '{}' is not a .dtr file",
                    base.display()
                ));
            }
        }

        // Target is a directory
        let dir = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
        if !dir.is_dir() {
            return Err(format!(
                "Project directory '{}' does not exist",
                dir.display()
            ));
        }

        let manifest_path = dir.join("datara.toml");
        let has_manifest = manifest_path.exists() && manifest_path.is_file();

        let manifest = if has_manifest {
            Some(DataraManifest::from_file(&manifest_path)?)
        } else {
            None
        };

        let project_name = if let Some(ref m) = manifest {
            m.package.name.clone()
        } else {
            dir.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("app")
                .to_string()
        };

        // Locate source files
        let src_dir = dir.join("src");
        let mut source_files = Vec::new();
        if src_dir.exists() && src_dir.is_dir() {
            Self::collect_dtr_files(&src_dir, &mut source_files)?;
        } else {
            Self::collect_dtr_files(&dir, &mut source_files)?;
        }

        if source_files.is_empty() {
            return Err(format!("No .dtr source files found in '{}'", dir.display()));
        }

        // Determine entry point (main.dtr prioritized, else first file)
        let main_idx = source_files
            .iter()
            .position(|p| p.file_name().and_then(|n| n.to_str()) == Some("main.dtr"));

        let entry_point = if let Some(idx) = main_idx {
            let main_file = source_files.remove(idx);
            source_files.insert(0, main_file.clone());
            main_file
        } else {
            source_files[0].clone()
        };

        // Locate tests, examples, benches
        let mut test_files = Vec::new();
        let tests_dir = dir.join("tests");
        if tests_dir.exists() && tests_dir.is_dir() {
            let _ = Self::collect_dtr_files(&tests_dir, &mut test_files);
        }

        let mut example_files = Vec::new();
        let examples_dir = dir.join("examples");
        if examples_dir.exists() && examples_dir.is_dir() {
            let _ = Self::collect_dtr_files(&examples_dir, &mut example_files);
        }

        let mut bench_files = Vec::new();
        let benches_dir = dir.join("benches");
        if benches_dir.exists() && benches_dir.is_dir() {
            let _ = Self::collect_dtr_files(&benches_dir, &mut bench_files);
        }

        let kind = if has_manifest {
            ProjectKind::ManifestProject(dir.clone())
        } else {
            ProjectKind::Directory(dir.clone())
        };

        Ok(ProjectLayout {
            root: dir,
            kind,
            manifest,
            name: project_name,
            entry_point,
            source_files,
            test_files,
            example_files,
            bench_files,
        })
    }

    fn collect_dtr_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
        if dir.is_dir() {
            let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Ignore target and hidden directories
                    if let Some(name) = path.file_name().and_then(|n| n.to_str())
                        && (name.starts_with('.')
                            || name == "target"
                            || name == "tests"
                            || name == "examples"
                            || name == "benches"
                            || name == "packages"
                            || name == "vendor"
                            || name == "node_modules"
                            || name == "dist"
                            || name == "build")
                    {
                        continue;
                    }
                    Self::collect_dtr_files(&path, files)?;
                } else if path.extension().and_then(|s| s.to_str()) == Some("dtr") {
                    files.push(path);
                }
            }
        }
        Ok(())
    }
}
