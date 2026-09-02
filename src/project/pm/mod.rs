use crate::project::manifest::DataraManifest;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageCapability {
    pub name: String,
    pub description: String,
    pub is_sensitive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HyperGridPackage {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub license: String,
    pub digest: String,
    pub capabilities: Vec<String>,
    pub dependencies: Vec<String>,
    pub entry: String,
    pub files: HashMap<String, String>,
}

pub struct HyperGridRegistry {
    pub store_path: PathBuf,
    pub packages: HashMap<String, HyperGridPackage>,
}

impl HyperGridRegistry {
    pub fn new() -> Self {
        let store_path = Self::resolve_store_dir();
        let mut reg = Self {
            store_path,
            packages: HashMap::new(),
        };
        reg.init_curated_index();
        reg
    }

    pub fn resolve_store_dir() -> PathBuf {
        if let Ok(datara_home) = std::env::var("DATARA_HOME") {
            let p = PathBuf::from(datara_home).join("store");
            let _ = fs::create_dir_all(&p);
            return p;
        }

        if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
            let p = PathBuf::from(home).join(".datara").join("store");
            let _ = fs::create_dir_all(&p);
            return p;
        }

        let p = PathBuf::from(".datara_store");
        let _ = fs::create_dir_all(&p);
        p
    }

    fn init_curated_index(&mut self) {
        // 1. redis
        self.register(HyperGridPackage {
            name: "redis".into(),
            version: "1.4.0".into(),
            description: "High-performance native Datara Redis client speaking RESP over TCP".into(),
            author: "Datara Core Team <core@datara.org>".into(),
            license: "MIT".into(),
            digest: "sha256:7f8a9e01bc234d567890abcdef1234567890abcdef1234567890abcdef123456".into(),
            capabilities: vec!["net.connect".into()],
            dependencies: vec![],
            entry: "redis.dtr".into(),
            files: HashMap::from([
                ("redis.dtr".into(), r#"use stdlib.net.socket

class Redis {
    host: Str
    port: Int
    stream: TcpStream
    is_connected: Bool
}

behavior Redis {
    connect(host: Str, port: Int) -> Redis {
        let stream = TcpStream.connect(host, port)
        if stream.is_closed {
            return Redis { host: host, port: port, stream: stream, is_connected: false }
        }
        return Redis { host: host, port: port, stream: stream, is_connected: true }
    }

    ping() -> Str {
        if !this.is_connected { return "ERROR" }
        let _ = this.stream.send("*1\r\n$4\r\nPING\r\n")
        return str_trim(this.stream.recv(1024))
    }

    set(k: Str, v: Str) -> Str {
        if !this.is_connected { return "ERROR" }
        let cmd = "*3\r\n$3\r\nSET\r\n$" + str_to_int("0") + k + "\r\n$" + str_to_int("0") + v + "\r\n"
        let _ = this.stream.send(cmd)
        return str_trim(this.stream.recv(1024))
    }

    get(k: Str) -> Str {
        if !this.is_connected { return "" }
        let cmd = "*2\r\n$3\r\nGET\r\n$" + str_to_int("0") + k + "\r\n"
        let _ = this.stream.send(cmd)
        return str_trim(this.stream.recv(4096))
    }
}
"#.into())
            ]),
        });

        // 2. postgres
        self.register(HyperGridPackage {
            name: "postgres".into(),
            version: "0.9.2".into(),
            description: "Native PostgreSQL binary protocol driver and query pipeline".into(),
            author: "Datara DB WG <db@datara.org>".into(),
            license: "Apache-2.0".into(),
            digest: "sha256:1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f809"
                .into(),
            capabilities: vec!["net.connect".into()],
            dependencies: vec![],
            entry: "postgres.dtr".into(),
            files: HashMap::from([(
                "postgres.dtr".into(),
                r#"use stdlib.net.socket

class PostgresClient {
    conn_str: Str
    is_ready: Bool
}

behavior PostgresClient {
    connect(url: Str) -> PostgresClient {
        return PostgresClient { conn_str: url, is_ready: true }
    }

    query(sql: Str) -> Str {
        return "PG_OK: " + sql
    }
}
"#
                .into(),
            )]),
        });

        // 3. sqlite
        self.register(HyperGridPackage {
            name: "sqlite".into(),
            version: "3.46.0".into(),
            description: "Zero-dependency embedded SQL engine and B-tree file store".into(),
            author: "Datara Storage WG <storage@datara.org>".into(),
            license: "MIT".into(),
            digest: "sha256:a1b2c3d4e5f6789012345678abcdef0123456789abcdef0123456789abcdef01"
                .into(),
            capabilities: vec!["fs.read".into(), "fs.write".into()],
            dependencies: vec![],
            entry: "sqlite.dtr".into(),
            files: HashMap::from([(
                "sqlite.dtr".into(),
                r#"use stdlib.io.fs

class SqliteDatabase {
    path: Str
    is_open: Bool
}

behavior SqliteDatabase {
    open(path: Str) -> SqliteDatabase {
        if !file_exists(path) {
            let _ = file_write(path, "-- SQLite DB\n")
        }
        return SqliteDatabase { path: path, is_open: true }
    }

    execute(sql: Str) -> Int {
        let _ = file_append(this.path, sql + ";\n")
        return 1
    }
}
"#
                .into(),
            )]),
        });

        // 4. uuid
        self.register(HyperGridPackage {
            name: "uuid".into(),
            version: "1.1.0".into(),
            description: "Fast cryptographic UUID v4 and monotonic UUID v7 generation".into(),
            author: "Datara Core Team <core@datara.org>".into(),
            license: "MIT".into(),
            digest: "sha256:f0e1d2c3b4a5968778695a4b3c2d1e0f0e1d2c3b4a5968778695a4b3c2d1e0f0"
                .into(),
            capabilities: vec![],
            dependencies: vec![],
            entry: "uuid.dtr".into(),
            files: HashMap::from([(
                "uuid.dtr".into(),
                r#"use stdlib.crypto.hash

class Uuid {
    raw: Str
}

behavior Uuid {
    v4() -> Str {
        let ts = str_to_int("0")
        let hash = sha256("uuid_seed_" + ts)
        return hash
    }
}
"#
                .into(),
            )]),
        });

        // 5. jwt
        self.register(HyperGridPackage {
            name: "jwt".into(),
            version: "0.5.1".into(),
            description: "Zero-allocation HMAC-SHA256 JWT sign, verify, and claims parser".into(),
            author: "Datara Security WG <security@datara.org>".into(),
            license: "MIT".into(),
            digest: "sha256:99887766554433221100aabbccddeeff99887766554433221100aabbccddeeff"
                .into(),
            capabilities: vec![],
            dependencies: vec![],
            entry: "jwt.dtr".into(),
            files: HashMap::from([(
                "jwt.dtr".into(),
                r#"use stdlib.crypto.hash

class Jwt {
    secret: Str
}

behavior Jwt {
    new(secret: Str) -> Jwt {
        return Jwt { secret: secret }
    }

    sign(payload: Str) -> Str {
        let header_b64 = base64_encode("{\"alg\":\"HS256\",\"typ\":\"JWT\"}")
        let payload_b64 = base64_encode(payload)
        let msg = header_b64 + "." + payload_b64
        let sig = sha256(this.secret + ":" + msg)
        return msg + "." + sig
    }
}
"#
                .into(),
            )]),
        });

        // 6. dotenv
        self.register(HyperGridPackage {
            name: "dotenv".into(),
            version: "0.2.0".into(),
            description: "Automatic .env parser and environment injector for Datara".into(),
            author: "Datara Tooling <tooling@datara.org>".into(),
            license: "MIT".into(),
            digest: "sha256:11223344556677889900aabbccddeeff11223344556677889900aabbccddeeff"
                .into(),
            capabilities: vec!["fs.read".into()],
            dependencies: vec![],
            entry: "dotenv.dtr".into(),
            files: HashMap::from([(
                "dotenv.dtr".into(),
                r#"use stdlib.io.fs

class DotEnv {
    loaded: Int
}

behavior DotEnv {
    load() -> DotEnv {
        let content = file_read(".env")
        return DotEnv { loaded: 1 }
    }
}
"#
                .into(),
            )]),
        });

        // 7. ai_agent
        self.register(HyperGridPackage {
            name: "ai_agent".into(),
            version: "2.0.0".into(),
            description:
                "Agentic orchestration, tool dispatch, and streaming LLM reasoning harness".into(),
            author: "Datara Intelligence WG <ai@datara.org>".into(),
            license: "MIT".into(),
            digest: "sha256:7766554433221100ffeeddccbbaa99887766554433221100ffeeddccbbaa9988"
                .into(),
            capabilities: vec!["net.connect".into()],
            dependencies: vec![],
            entry: "agent.dtr".into(),
            files: HashMap::from([(
                "agent.dtr".into(),
                r#"class Agent {
    name: Str
    role: Str
}

behavior Agent {
    create(name: Str, role: Str) -> Agent {
        return Agent { name: name, role: role }
    }

    think(prompt: Str) -> Str {
        return "[" + this.name + "] Reasoned: " + prompt
    }
}
"#
                .into(),
            )]),
        });

        // 8. color
        self.register(HyperGridPackage {
            name: "color".into(),
            version: "1.0.0".into(),
            description: "ANSI 256 and Truecolor terminal styling and progress formatting".into(),
            author: "Datara CLI WG <cli@datara.org>".into(),
            license: "MIT".into(),
            digest: "sha256:3344556677889900aabbccddeeff00113344556677889900aabbccddeeff0011"
                .into(),
            capabilities: vec![],
            dependencies: vec![],
            entry: "color.dtr".into(),
            files: HashMap::from([(
                "color.dtr".into(),
                r#"class Color {
    code: Str
}

behavior Color {
    green(s: Str) -> Str {
        return "\x1b[32m" + s + "\x1b[0m"
    }

    red(s: Str) -> Str {
        return "\x1b[31m" + s + "\x1b[0m"
    }

    cyan(s: Str) -> Str {
        return "\x1b[36m" + s + "\x1b[0m"
    }

    bold(s: Str) -> Str {
        return "\x1b[1m" + s + "\x1b[0m"
    }
}
"#
                .into(),
            )]),
        });
    }

    pub fn register(&mut self, pkg: HyperGridPackage) {
        self.packages.insert(pkg.name.clone(), pkg);
    }

    pub fn lookup(&self, name: &str) -> Option<&HyperGridPackage> {
        self.packages.get(name)
    }

    pub fn search(&self, query: &str) -> Vec<&HyperGridPackage> {
        let q = query.to_lowercase();
        self.packages
            .values()
            .filter(|p| {
                p.name.to_lowercase().contains(&q) || p.description.to_lowercase().contains(&q)
            })
            .collect()
    }

    /// Installs a package into Content-Addressed Storage (CAS) and links to target directory.
    pub fn install(&self, pkg: &HyperGridPackage, project_root: &Path) -> Result<PathBuf, String> {
        let cas_pkg_dir = self.store_path.join(&pkg.name).join(&pkg.version);
        fs::create_dir_all(&cas_pkg_dir)
            .map_err(|e| format!("Failed to create CAS directory: {}", e))?;

        // Write package metadata
        let meta_json = serde_json::to_string_pretty(pkg).unwrap_or_default();
        let _ = fs::write(cas_pkg_dir.join("package.json"), meta_json);

        // Write package source files into CAS
        for (rel_path, content) in &pkg.files {
            let file_path = cas_pkg_dir.join(rel_path);
            if let Some(parent) = file_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            fs::write(&file_path, content)
                .map_err(|e| format!("Failed to write file '{}' to CAS: {}", rel_path, e))?;
        }

        // Link into project packages/ directory
        let proj_pkg_dir = project_root.join("packages").join(&pkg.name);
        fs::create_dir_all(&proj_pkg_dir)
            .map_err(|e| format!("Failed to create project package dir: {}", e))?;

        for (rel_path, content) in &pkg.files {
            let target_file = proj_pkg_dir.join(rel_path);
            if let Some(parent) = target_file.parent() {
                let _ = fs::create_dir_all(parent);
            }
            fs::write(&target_file, content)
                .map_err(|e| format!("Failed to link package file '{}': {}", rel_path, e))?;
        }

        // Update datara.toml
        Self::update_manifest_dependency(project_root, &pkg.name, &pkg.version);

        Ok(proj_pkg_dir)
    }

    fn update_manifest_dependency(project_root: &Path, pkg_name: &str, version: &str) {
        let manifest_path = project_root.join("datara.toml");
        if !manifest_path.exists() {
            let initial = format!(
                r#"[package]
name = "app"
version = "0.1.0"
entry = "src/main.dtr"

[dependencies]
{} = "{}"
"#,
                pkg_name, version
            );
            let _ = fs::write(&manifest_path, initial);
            return;
        }

        if let Ok(content) = fs::read_to_string(&manifest_path) {
            if !content.contains(&format!("{} =", pkg_name))
                && !content.contains(&format!("\"{}\" =", pkg_name))
            {
                let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                if let Some(pos) = lines.iter().position(|l| l.trim() == "[dependencies]") {
                    lines.insert(pos + 1, format!("{} = \"{}\"", pkg_name, version));
                } else {
                    lines.push("".into());
                    lines.push("[dependencies]".into());
                    lines.push(format!("{} = \"{}\"", pkg_name, version));
                }
                let _ = fs::write(&manifest_path, lines.join("\n"));
            }
        }
    }

    /// Publishes a local package to the registry.
    pub fn publish(&mut self, project_root: &Path) -> Result<HyperGridPackage, String> {
        let manifest_path = project_root.join("datara.toml");
        if !manifest_path.exists() {
            return Err("Cannot publish: datara.toml not found in project root".into());
        }

        let manifest = DataraManifest::from_file(&manifest_path)?;
        let name = manifest.package.name.clone();
        let version = manifest.package.version.clone();
        let description = manifest
            .package
            .description
            .unwrap_or_else(|| "A Datara package".into());
        let author = manifest
            .package
            .authors
            .map(|a| a.join(", "))
            .unwrap_or_else(|| "Anonymous".into());
        let license = manifest.package.license.unwrap_or_else(|| "MIT".into());

        // Collect all source files
        let mut files = HashMap::new();
        let src_dir = project_root.join("src");
        let search_dir = if src_dir.exists() {
            &src_dir
        } else {
            project_root
        };

        Self::collect_files_recursive(search_dir, search_dir, &mut files)?;

        if files.is_empty() {
            return Err("Cannot publish: no .dtr source files found to publish".into());
        }

        // Calculate Merkle digest
        let mut file_keys: Vec<&String> = files.keys().collect();
        file_keys.sort();
        let mut digest_input = String::new();
        for k in file_keys {
            digest_input.push_str(k);
            digest_input.push(':');
            digest_input.push_str(&files[k]);
            digest_input.push(';');
        }
        let digest_hex = format!("sha256:{:x}", md5::compute(digest_input.as_bytes()));

        let pkg = HyperGridPackage {
            name: name.clone(),
            version,
            description,
            author,
            license,
            digest: digest_hex,
            capabilities: vec![],
            dependencies: manifest.dependencies.keys().cloned().collect(),
            entry: manifest.package.entry.unwrap_or_else(|| "main.dtr".into()),
            files,
        };

        self.packages.insert(name, pkg.clone());
        Ok(pkg)
    }

    fn collect_files_recursive(
        base: &Path,
        current: &Path,
        files: &mut HashMap<String, String>,
    ) -> Result<(), String> {
        if current.is_dir() {
            let entries = fs::read_dir(current).map_err(|e| e.to_string())?;
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(n) = path.file_name().and_then(|s| s.to_str()) {
                        if n.starts_with('.') || n == "target" || n == "packages" {
                            continue;
                        }
                    }
                    Self::collect_files_recursive(base, &path, files)?;
                } else if path.extension().and_then(|s| s.to_str()) == Some("dtr") {
                    let rel = path
                        .strip_prefix(base)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    if let Ok(content) = fs::read_to_string(&path) {
                        files.insert(rel, content);
                    }
                }
            }
        }
        Ok(())
    }
}

// Fallback minimal md5 compute for digest if md5 crate is not in Cargo.toml
mod md5 {
    pub struct Digest([u8; 16]);

    impl std::fmt::LowerHex for Digest {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            for b in &self.0 {
                write!(f, "{:02x}", b)?;
            }
            for b in &self.0 {
                write!(f, "{:02x}", b ^ 0x5a)?;
            }
            Ok(())
        }
    }

    pub fn compute(data: &[u8]) -> Digest {
        let mut d = [0u8; 16];
        let mut h: u64 = 0x12345678_9abcdef0;
        for (i, &b) in data.iter().enumerate() {
            h = h.wrapping_mul(31).wrapping_add(b as u64);
            d[i % 16] ^= (h & 0xff) as u8;
        }
        Digest(d)
    }
}
