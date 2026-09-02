use super::manifest::DataraManifest;
use std::fs;
use std::path::Path;

pub struct ProjectInitializer;

impl ProjectInitializer {
    /// Initializes a standard Level 3 Datara project
    pub fn init(name: Option<&str>, target_dir: &Path) -> Result<(), String> {
        let dir_name = if let Some(n) = name {
            n.to_string()
        } else {
            target_dir
                .canonicalize()
                .ok()
                .and_then(|p| {
                    p.file_name()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| "datara_app".to_string())
        };

        let project_root = if name.is_some() {
            target_dir.join(&dir_name)
        } else {
            target_dir.to_path_buf()
        };

        if !project_root.exists() {
            fs::create_dir_all(&project_root).map_err(|e| {
                format!(
                    "Failed to create project directory '{}': {}",
                    project_root.display(),
                    e
                )
            })?;
        }

        // 1. datara.toml manifest
        let manifest_path = project_root.join("datara.toml");
        if !manifest_path.exists() {
            let content = DataraManifest::default_template(&dir_name);
            fs::write(&manifest_path, content)
                .map_err(|e| format!("Failed to write datara.toml: {}", e))?;
        }

        // 2. src/main.dtr
        let src_dir = project_root.join("src");
        fs::create_dir_all(&src_dir).map_err(|e| format!("Failed to create src dir: {}", e))?;
        let main_path = src_dir.join("main.dtr");
        if !main_path.exists() {
            let main_content = format!(
                "// Main entry point for {}\nfn main() {{\n    out \"Hello from Datara!\"\n}}\n",
                dir_name
            );
            fs::write(&main_path, main_content)
                .map_err(|e| format!("Failed to write src/main.dtr: {}", e))?;
        }

        // 3. tests/test_main.dtr
        let tests_dir = project_root.join("tests");
        fs::create_dir_all(&tests_dir).map_err(|e| format!("Failed to create tests dir: {}", e))?;
        let test_path = tests_dir.join("test_main.dtr");
        if !test_path.exists() {
            let test_content = "// Integration test for application\nfn test_addition() -> Int {\n    return 10 + 20\n}\n\nfn main() {\n    let res = test_addition()\n    if res == 30 {\n        out \"PASS: test_addition\"\n    } else {\n        err \"FAIL: test_addition\"\n    }\n}\n";
            fs::write(&test_path, test_content)
                .map_err(|e| format!("Failed to write tests/test_main.dtr: {}", e))?;
        }

        // 4. examples/demo.dtr
        let examples_dir = project_root.join("examples");
        fs::create_dir_all(&examples_dir)
            .map_err(|e| format!("Failed to create examples dir: {}", e))?;
        let example_path = examples_dir.join("demo.dtr");
        if !example_path.exists() {
            let example_content = "// Example usage demo\nclass Greeter {\n    name String\n}\n\nbehavior Greeter {\n    fn greet() {\n        out \"Welcome to \" + self.name\n    }\n}\n\nfn main() {\n    let app = Greeter { name: \"Datara\" }\n    app.greet()\n}\n";
            fs::write(&example_path, example_content)
                .map_err(|e| format!("Failed to write examples/demo.dtr: {}", e))?;
        }

        // 5. .gitignore
        let gitignore_path = project_root.join(".gitignore");
        if !gitignore_path.exists() {
            let gitignore_content = "target/\n*.exe\n*.obj\n*.pdb\n*.pgo.json\n";
            let _ = fs::write(&gitignore_path, gitignore_content);
        }

        println!(
            "Created Datara package '{}' at '{}'",
            dir_name,
            project_root.display()
        );
        Ok(())
    }

    /// Initializes a Datara Community Library package
    pub fn init_lib(name: Option<&str>, target_dir: &Path) -> Result<(), String> {
        let dir_name = if let Some(n) = name {
            n.to_string()
        } else {
            target_dir
                .canonicalize()
                .ok()
                .and_then(|p| {
                    p.file_name()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| "datara_lib".to_string())
        };

        let project_root = if name.is_some() {
            target_dir.join(&dir_name)
        } else {
            target_dir.to_path_buf()
        };

        if !project_root.exists() {
            fs::create_dir_all(&project_root).map_err(|e| {
                format!(
                    "Failed to create library directory '{}': {}",
                    project_root.display(),
                    e
                )
            })?;
        }

        // 1. datara.toml library manifest
        let manifest_path = project_root.join("datara.toml");
        if !manifest_path.exists() {
            let content = format!(
                "[package]\nname = \"{}\"\nversion = \"0.1.0\"\ntype = \"lib\"\nauthors = []\ndescription = \"A high-performance Datara community library\"\n\n[exports]\nroot = \"src/lib.dtr\"\n",
                dir_name
            );
            fs::write(&manifest_path, content)
                .map_err(|e| format!("Failed to write datara.toml: {}", e))?;
        }

        // 2. src/lib.dtr
        let src_dir = project_root.join("src");
        fs::create_dir_all(&src_dir).map_err(|e| format!("Failed to create src dir: {}", e))?;
        let lib_path = src_dir.join("lib.dtr");
        if !lib_path.exists() {
            let lib_content = format!(
                "// Datara Library: {}\nclass Helper {{\n    scale_factor: Int\n}}\n\nbehavior Helper {{\n    multiply(val: Int) -> Int => val * this.scale_factor\n}}\n\nfn create_helper(scale: Int) -> Helper {{\n    return Helper {{ scale_factor: scale }}\n}}\n",
                dir_name
            );
            fs::write(&lib_path, lib_content)
                .map_err(|e| format!("Failed to write src/lib.dtr: {}", e))?;
        }

        // 3. tests/test_lib.dtr
        let tests_dir = project_root.join("tests");
        fs::create_dir_all(&tests_dir).map_err(|e| format!("Failed to create tests dir: {}", e))?;
        let test_path = tests_dir.join("test_lib.dtr");
        if !test_path.exists() {
            let test_content = format!(
                "// Integration tests for library {}\nuse src.lib\n\nfn main() {{\n    let helper = create_helper(5)\n    let res = helper.multiply(10)\n    if res == 50 {{\n        out \"PASS: library tests\"\n    }}\n}}\n",
                dir_name
            );
            fs::write(&test_path, test_content)
                .map_err(|e| format!("Failed to write tests/test_lib.dtr: {}", e))?;
        }

        // 4. README.md
        let readme_path = project_root.join("README.md");
        if !readme_path.exists() {
            let readme_content = format!(
                "# {}\n\nA high-performance Datara community library.\n\n## Usage\n\n```datara\nuse {}\n\nfn main() {{\n    let helper = create_helper(2)\n    out helper.multiply(21)\n}}\n```\n",
                dir_name, dir_name
            );
            let _ = fs::write(&readme_path, readme_content);
        }

        // 5. .gitignore
        let gitignore_path = project_root.join(".gitignore");
        if !gitignore_path.exists() {
            let gitignore_content = "target/\n*.exe\n*.obj\n*.dll\n*.lib\n";
            let _ = fs::write(&gitignore_path, gitignore_content);
        }

        println!(
            "[Forgen] Successfully created Datara library '{}' at '{}'",
            dir_name,
            project_root.display()
        );
        Ok(())
    }
}
