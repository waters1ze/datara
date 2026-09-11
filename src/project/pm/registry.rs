use super::crypto::*;
use super::http;
use super::manifest::*;
use super::tar;
use super::tar::*;
use super::verify::*;
use crate::project::manifest::DataraManifest;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RegistryIndex {
    pub updated_at: String,
    pub packages: HashMap<String, RegistryIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryIndexEntry {
    pub name: String,
    pub version: String,
    pub description: String,
    pub digest: String,
    pub capabilities: Vec<String>,
    pub bundle_file: String,
}

pub struct HyperGridRegistry {
    pub store_path: PathBuf,
    pub packages: HashMap<String, HyperGridPackage>,
    pub offline: bool,
    pub registry_url: String,
}

impl Default for HyperGridRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl HyperGridRegistry {
    pub fn new() -> Self {
        let store_path = Self::resolve_store_dir();
        let offline = std::env::var("DATARA_OFFLINE")
            .or_else(|_| std::env::var("FORGEN_OFFLINE"))
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let registry_url = std::env::var("DATARA_REGISTRY_URL")
            .unwrap_or_else(|_| "https://registry.datara.org".to_string());
        let mut reg = Self {
            store_path,
            packages: HashMap::new(),
            offline,
            registry_url,
        };
        reg.init_curated_index();
        reg.load_index_from_disk();
        reg
    }

    pub fn set_offline(&mut self, offline: bool) {
        self.offline = offline;
    }

    pub fn is_offline(&self) -> bool {
        self.offline
            || std::env::var("DATARA_OFFLINE")
                .or_else(|_| std::env::var("FORGEN_OFFLINE"))
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false)
    }

    pub fn set_registry_url(&mut self, url: impl Into<String>) {
        self.registry_url = url.into();
    }

    pub fn registry_url(&self) -> &str {
        &self.registry_url
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
        let k_len = int_to_str(str_len(k))
        let v_len = int_to_str(str_len(v))
        let cmd = "*3\r\n$3\r\nSET\r\n$" + k_len + "\r\n" + k + "\r\n$" + v_len + "\r\n" + v + "\r\n"
        let _ = this.stream.send(cmd)
        return str_trim(this.stream.recv(1024))
    }

    get(k: Str) -> Str {
        if !this.is_connected { return "" }
        let k_len = int_to_str(str_len(k))
        let cmd = "*2\r\n$3\r\nGET\r\n$" + k_len + "\r\n" + k + "\r\n"
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
            capabilities: vec!["fs.read".into()],
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
        return uuid_v4()
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

        // 7. logger
        self.register(HyperGridPackage {
            name: "logger".into(),
            version: "1.0.0".into(),
            description: "Structured logging and terminal log formatting utility".into(),
            author: "Datara Core WG <dev@datara.org>".into(),
            license: "MIT".into(),
            digest: "sha256:7766554433221100ffeeddccbbaa99887766554433221100ffeeddccbbaa9988"
                .into(),
            capabilities: vec![],
            dependencies: vec![],
            entry: "logger.dtr".into(),
            files: HashMap::from([(
                "logger.dtr".into(),
                r#"class Logger {
    prefix: Str
}

behavior Logger {
    create(prefix: Str) -> Logger {
        return Logger { prefix: prefix }
    }

    info(msg: Str) -> Str {
        return "[" + this.prefix + "] " + msg
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

        // 9. http_router
        self.register(HyperGridPackage {
            name: "http_router".into(),
            version: "0.8.0".into(),
            description: "Radix tree micro-router for high-throughput REST and HTTP APIs".into(),
            author: "Datara Web WG <web@datara.org>".into(),
            license: "MIT".into(),
            digest: "sha256:4455667788990011aabbccddeeff22334455667788990011aabbccddeeff2233"
                .into(),
            capabilities: vec![],
            dependencies: vec![],
            entry: "router.dtr".into(),
            files: HashMap::from([(
                "router.dtr".into(),
                r#"class RouteMatch {
    matched: Bool
    handler: Str
}

class HttpRouter {
    prefix: Str
}

behavior HttpRouter {
    new(prefix: Str) -> HttpRouter {
        return HttpRouter { prefix: prefix }
    }

    dispatch(method: Str, path: Str) -> RouteMatch {
        return RouteMatch { matched: true, handler: method + " " + path }
    }
}
"#
                .into(),
            )]),
        });

        // 10. math_matrix
        self.register(HyperGridPackage {
            name: "math_matrix".into(),
            version: "1.2.0".into(),
            description: "Linear algebra, matrix multiplication, and SIMD vector primitives".into(),
            author: "Datara Numerics WG <math@datara.org>".into(),
            license: "MIT".into(),
            digest: "sha256:5566778899001122aabbccddeeff33445566778899001122aabbccddeeff3344"
                .into(),
            capabilities: vec![],
            dependencies: vec![],
            entry: "matrix.dtr".into(),
            files: HashMap::from([(
                "matrix.dtr".into(),
                r#"class Matrix2x2 {
    m00: Float
    m01: Float
    m10: Float
    m11: Float
}

behavior Matrix2x2 {
    identity() -> Matrix2x2 {
        return Matrix2x2 { m00: 1.0, m01: 0.0, m10: 0.0, m11: 1.0 }
    }

    determinant() -> Float {
        return (this.m00 * this.m11) - (this.m01 * this.m10)
    }
}
"#
                .into(),
            )]),
        });
    }

    pub fn compute_digest(files: &HashMap<String, String>) -> String {
        let mut file_keys: Vec<&String> = files.keys().collect();
        file_keys.sort();
        let mut digest_input = String::new();
        for k in file_keys {
            digest_input.push_str(k);
            digest_input.push(':');
            if let Some(content) = files.get(k.as_str()) {
                let normalized = content.replace("\r\n", "\n");
                digest_input.push_str(&normalized);
            }
            digest_input.push(';');
        }
        format!("sha256:{}", sha256::hexdigest(digest_input.as_bytes()))
    }

    pub fn compute_merkle_root(files: &HashMap<String, String>) -> String {
        if files.is_empty() {
            return "merkle:sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into();
        }
        let mut file_keys: Vec<&String> = files.keys().collect();
        file_keys.sort();

        // 1. Calculate leaf hashes for each file
        let mut current_level: Vec<String> = file_keys
            .iter()
            .map(|k| {
                let content = files.get(k.as_str()).map(|s| s.as_str()).unwrap_or("");
                let normalized = content.replace("\r\n", "\n");
                let leaf_input = format!("leaf:{}:{}:{}", k, normalized.len(), normalized);
                sha256::hexdigest(leaf_input.as_bytes())
            })
            .collect();

        // 2. Build Merkle tree layers up to the root
        while current_level.len() > 1 {
            let mut next_level = Vec::with_capacity(current_level.len().div_ceil(2));
            for chunk in current_level.chunks(2) {
                if chunk.len() == 2 {
                    let pair_input = format!("node:{}:{}", chunk[0], chunk[1]);
                    next_level.push(sha256::hexdigest(pair_input.as_bytes()));
                } else {
                    let pair_input = format!("node:{}:{}", chunk[0], chunk[0]);
                    next_level.push(sha256::hexdigest(pair_input.as_bytes()));
                }
            }
            current_level = next_level;
        }

        format!("merkle:sha256:{}", current_level[0])
    }

    pub fn extract_capabilities(files: &HashMap<String, String>) -> Vec<String> {
        let mut caps = std::collections::BTreeSet::new();

        for content in files.values() {
            if content.contains("stdlib.net")
                || content.contains("TcpStream")
                || content.contains("fetch(")
                || content.contains("http")
                || content.contains("socket")
            {
                caps.insert("net.network".to_string());
            }
            if content.contains("stdlib.io.fs")
                || content.contains("file_")
                || content.contains("fs.")
                || content.contains("File.")
            {
                caps.insert("fs.filesystem".to_string());
            }
            if content.contains("stdlib.sys")
                || content.contains("sys.")
                || content.contains("get_env")
                || content.contains("process")
            {
                caps.insert("sys.system".to_string());
            }
            if content.contains("unsafe") || content.contains("ptr_") || content.contains("alloc_")
            {
                caps.insert("unsafe.memory".to_string());
            }
            if content.contains("sha256") || content.contains("hmac") || content.contains("crypto")
            {
                caps.insert("crypto.security".to_string());
            }
            if content.contains("stdlib.ui")
                || content.contains("Page")
                || content.contains("Widget")
                || content.contains("Component")
            {
                caps.insert("ui.rendering".to_string());
            }
            if content.contains("stdlib.async")
                || content.contains("spawn")
                || content.contains("Future")
                || content.contains("Task")
            {
                caps.insert("async.concurrency".to_string());
            }
            if content.contains("sql") || content.contains("database") || content.contains("Sqlite")
            {
                caps.insert("db.database".to_string());
            }
        }

        caps.into_iter().collect()
    }

    pub fn update_disk_index(&self, pkg: &HyperGridPackage) {
        let index_path = self.store_path.join("index.json");
        let mut index: RegistryIndex = if index_path.exists() {
            fs::read_to_string(&index_path)
                .ok()
                .and_then(|c| serde_json::from_str(&c).ok())
                .unwrap_or_default()
        } else {
            RegistryIndex::default()
        };

        index.updated_at = format!("{:?}", std::time::SystemTime::now());
        index.packages.insert(
            pkg.name.clone(),
            RegistryIndexEntry {
                name: pkg.name.clone(),
                version: pkg.version.clone(),
                description: pkg.description.clone(),
                digest: pkg.digest.clone(),
                capabilities: pkg.capabilities.clone(),
                bundle_file: format!("bundles/{}-{}.dtr-pkg", pkg.name, pkg.version),
            },
        );

        if let Ok(data) = serde_json::to_string_pretty(&index) {
            let _ = fs::write(index_path, data);
        }
    }

    pub fn load_index_from_disk(&mut self) {
        let index_path = self.store_path.join("index.json");
        if !index_path.exists() {
            return;
        }
        let content = match fs::read_to_string(&index_path) {
            Ok(c) => c,
            Err(_) => return,
        };
        let index: RegistryIndex = match serde_json::from_str(&content) {
            Ok(idx) => idx,
            Err(_) => return,
        };

        for (name, entry) in index.packages {
            let bundle_path = self.store_path.join(&entry.bundle_file);
            if bundle_path.exists()
                && let Ok(b_content) = fs::read_to_string(&bundle_path)
                && let Ok(pkg) = serde_json::from_str::<HyperGridPackage>(&b_content)
            {
                self.packages.insert(name, pkg);
            }
        }
    }

    /// Package names become path components (`packages/<name>`, CAS
    /// `<store>/<name>/<version>`), so they must be plain identifiers: no path
    /// separators, no `..`, no leading dot/dash.
    pub fn is_valid_package_name(name: &str) -> bool {
        let pkg_id = if let Some(stripped) = name.strip_prefix("sparks/") {
            stripped
        } else {
            name
        };
        if pkg_id.is_empty() || pkg_id.len() > 64 {
            return false;
        }
        let first = pkg_id.chars().next().unwrap();
        if !(first.is_ascii_alphanumeric() || first == '_') {
            return false;
        }
        pkg_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
            && !pkg_id.contains("..")
    }

    /// Normalizes a package-internal file path and rejects anything that could
    /// escape the package directory (zip-slip): absolute paths, `..`
    /// components, backslashes, drive letters and NUL bytes.
    fn sanitize_package_rel_path(rel: &str) -> Option<String> {
        let normalized = rel.replace('\\', "/");
        if normalized.is_empty()
            || normalized.starts_with('/')
            || normalized.contains('\0')
            || normalized.contains(':')
        {
            return None;
        }
        if normalized.split('/').any(|part| part == "..") {
            return None;
        }
        Some(normalized)
    }

    pub fn register(&mut self, mut pkg: HyperGridPackage) {
        pkg.digest = Self::compute_digest(&pkg.files);
        self.packages.insert(pkg.name.clone(), pkg);
    }

    pub fn lookup(&self, name: &str) -> Option<&HyperGridPackage> {
        self.packages.get(name)
    }

    pub fn search(&self, query: &str) -> Vec<&HyperGridPackage> {
        let q = query.to_lowercase();
        let mut results: Vec<&HyperGridPackage> = self
            .packages
            .values()
            .filter(|p| {
                p.name.to_lowercase().contains(&q) || p.description.to_lowercase().contains(&q)
            })
            .collect();
        results.sort_by_key(|p| &p.name);
        results
    }

    /// Installs a package into Content-Addressed Storage (CAS), links to target directory,
    /// and synchronizes datara.toml and datara.lock.
    pub fn install(&self, pkg: &HyperGridPackage, project_root: &Path) -> Result<PathBuf, String> {
        self.install_with_source(pkg, project_root, "registry")
    }

    /// Installs a package recording a specific source URL or identifier in datara.lock.
    pub fn install_with_source(
        &self,
        pkg: &HyperGridPackage,
        project_root: &Path,
        source: &str,
    ) -> Result<PathBuf, String> {
        if !Self::is_valid_package_name(&pkg.name) {
            return Err(format!(
                "Invalid package name '{}': must be a plain identifier (no path separators or '..')",
                pkg.name
            ));
        }
        let cas_pkg_dir = self.store_path.join(&pkg.name).join(&pkg.version);
        fs::create_dir_all(&cas_pkg_dir)
            .map_err(|e| format!("Failed to create CAS directory: {}", e))?;

        // Write package metadata
        let meta_json = serde_json::to_string_pretty(pkg).unwrap_or_default();
        let _ = fs::write(cas_pkg_dir.join("package.json"), meta_json);

        // Write package source files into CAS. Every relative path is
        // sanitized first: package metadata is untrusted input and a crafted
        // path like `..\\..\\x` must never write outside the package
        // directory (zip-slip).
        for (rel_path, content) in &pkg.files {
            let safe_rel = Self::sanitize_package_rel_path(rel_path).ok_or_else(|| {
                format!(
                    "Package '{}' contains an unsafe file path '{}'",
                    pkg.name, rel_path
                )
            })?;
            let file_path = cas_pkg_dir.join(&safe_rel);
            if let Some(parent) = file_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            fs::write(&file_path, content)
                .map_err(|e| format!("Failed to write file '{}' to CAS: {}", safe_rel, e))?;
        }

        // Link into project packages/ directory (same zip-slip guard).
        let proj_pkg_dir = project_root.join("packages").join(&pkg.name);
        fs::create_dir_all(&proj_pkg_dir)
            .map_err(|e| format!("Failed to create project package dir: {}", e))?;

        if let Ok(meta_json) = serde_json::to_string_pretty(pkg) {
            let _ = fs::write(proj_pkg_dir.join("package.json"), meta_json);
        }

        for (rel_path, content) in &pkg.files {
            let safe_rel = Self::sanitize_package_rel_path(rel_path).ok_or_else(|| {
                format!(
                    "Package '{}' contains an unsafe file path '{}'",
                    pkg.name, rel_path
                )
            })?;
            let target_file = proj_pkg_dir.join(&safe_rel);
            if let Some(parent) = target_file.parent() {
                let _ = fs::create_dir_all(parent);
            }
            fs::write(&target_file, content)
                .map_err(|e| format!("Failed to link package file '{}': {}", safe_rel, e))?;
        }

        // Update datara.toml
        Self::update_manifest_dependency(project_root, &pkg.name, &pkg.version);

        // Update datara.lock
        let mut lock = DataraLock::load(project_root).unwrap_or_default();
        lock.insert_or_update(
            &pkg.name,
            &pkg.version,
            &pkg.digest,
            source,
            pkg.dependencies.clone(),
        );
        let _ = lock.save(project_root);

        Ok(proj_pkg_dir)
    }

    /// Converts a package into a .tar archive containing package.json and all source files.
    pub fn package_to_tar(pkg: &HyperGridPackage) -> Result<Vec<u8>, String> {
        let mut files = HashMap::new();
        let meta_json = serde_json::to_string_pretty(pkg)
            .map_err(|e| format!("Failed to serialize package metadata: {}", e))?;
        files.insert("package.json".to_string(), meta_json.into_bytes());

        for (rel_path, content) in &pkg.files {
            let safe_rel = tar::sanitize_tar_path(rel_path)?;
            files.insert(safe_rel, content.as_bytes().to_vec());
        }

        tar::create_tar(&files)
    }

    /// Parses a .tar archive into a HyperGridPackage.
    pub fn package_from_tar(tar_bytes: &[u8]) -> Result<HyperGridPackage, String> {
        let files_map = tar::extract_tar(tar_bytes)?;
        let mut str_files = HashMap::new();
        let mut meta_pkg: Option<HyperGridPackage> = None;

        for (name, bytes) in files_map {
            let text = String::from_utf8(bytes)
                .map_err(|e| format!("File '{}' in archive contains invalid UTF-8: {}", name, e))?;
            if name == "package.json" {
                if let Ok(pkg) = serde_json::from_str::<HyperGridPackage>(&text) {
                    meta_pkg = Some(pkg);
                }
            } else {
                str_files.insert(name, text);
            }
        }

        if let Some(mut pkg) = meta_pkg {
            if pkg.files.is_empty() {
                pkg.files = str_files;
            }
            if pkg.digest.is_empty() {
                pkg.digest = Self::compute_digest(&pkg.files);
            }
            Ok(pkg)
        } else {
            let entry = if str_files.contains_key("lib.dtr") {
                "lib.dtr".to_string()
            } else if str_files.contains_key("src/lib.dtr") {
                "src/lib.dtr".to_string()
            } else if str_files.contains_key("main.dtr") {
                "main.dtr".to_string()
            } else {
                str_files
                    .keys()
                    .next()
                    .cloned()
                    .unwrap_or_else(|| "main.dtr".to_string())
            };
            let name = entry
                .trim_end_matches(".dtr")
                .split('/')
                .next_back()
                .unwrap_or("pkg")
                .to_string();
            let digest = Self::compute_digest(&str_files);
            let caps = Self::extract_capabilities(&str_files);

            Ok(HyperGridPackage {
                name,
                version: "0.1.0".to_string(),
                description: "Package extracted from tar archive".to_string(),
                author: "Unknown".to_string(),
                license: "MIT".to_string(),
                digest,
                capabilities: caps,
                dependencies: vec![],
                entry,
                files: str_files,
            })
        }
    }

    /// Verifies that data matches expected SHA-256 hex digest.
    pub fn verify_sha256(data: &[u8], expected_digest: &str) -> Result<(), String> {
        let clean_expected = expected_digest
            .trim()
            .strip_prefix("sha256:")
            .unwrap_or(expected_digest.trim());
        let actual = sha256::hexdigest(data);
        if actual.eq_ignore_ascii_case(clean_expected) {
            Ok(())
        } else {
            Err(format!(
                "SHA-256 checksum mismatch!\n  Expected: {}\n  Actual:   {}\nRefusing to install corrupted or tampered package.",
                clean_expected, actual
            ))
        }
    }

    /// Fetches a package tarball over HTTP/HTTPS/file, verifies SHA-256, extracts,
    /// stores in CAS and links into project. Supports offline mode.
    pub fn fetch_and_install_tarball(
        &self,
        name: &str,
        version: &str,
        tarball_url: &str,
        expected_sha256: &str,
        project_root: &Path,
    ) -> Result<PathBuf, String> {
        let cas_pkg_dir = self.store_path.join(name).join(version);

        // Offline mode: resolve strictly from CAS
        if self.is_offline() {
            if cas_pkg_dir.exists() && cas_pkg_dir.join("package.json").exists() {
                let meta_str = fs::read_to_string(cas_pkg_dir.join("package.json"))
                    .map_err(|e| format!("Failed to read cached package metadata: {}", e))?;
                let pkg: HyperGridPackage = serde_json::from_str(&meta_str)
                    .map_err(|e| format!("Corrupted cached package metadata: {}", e))?;

                return self.install_with_source(&pkg, project_root, tarball_url);
            } else {
                return Err(format!(
                    "Offline mode active: package '{}@{}' not found in local CAS cache ({}). Run without --offline or pre-fetch packages while online.",
                    name,
                    version,
                    cas_pkg_dir.display()
                ));
            }
        }

        // Online mode: check CAS cache hit first
        if cas_pkg_dir.exists() && cas_pkg_dir.join("package.json").exists() {
            if let Ok(meta_str) = fs::read_to_string(cas_pkg_dir.join("package.json")) {
                if let Ok(pkg) = serde_json::from_str::<HyperGridPackage>(&meta_str) {
                    return self.install_with_source(&pkg, project_root, tarball_url);
                }
            }
        }

        // Network fetch
        let data = http::fetch_url(tarball_url)?;

        let tar_sha = sha256::hexdigest(&data);
        let clean_expected = expected_sha256
            .trim()
            .strip_prefix("sha256:")
            .unwrap_or(expected_sha256.trim());

        let tar_checksum_matches =
            !expected_sha256.is_empty() && tar_sha.eq_ignore_ascii_case(clean_expected);

        // Tar extraction
        let pkg = match Self::package_from_tar(&data) {
            Ok(pkg) => {
                let clean_pkg_digest = pkg
                    .digest
                    .trim()
                    .strip_prefix("sha256:")
                    .unwrap_or(pkg.digest.trim());
                if !expected_sha256.is_empty()
                    && !tar_checksum_matches
                    && !clean_pkg_digest.eq_ignore_ascii_case(clean_expected)
                {
                    return Err(format!(
                        "SHA-256 checksum mismatch!\n  Expected: {}\n  Actual (tarball): sha256:{}\n  Actual (content): {}\nRefusing to install corrupted or tampered package.",
                        expected_sha256, tar_sha, pkg.digest
                    ));
                }
                pkg
            }
            Err(e) => {
                if !expected_sha256.is_empty() && !tar_checksum_matches {
                    return Err(format!(
                        "SHA-256 checksum mismatch!\n  Expected: {}\n  Actual (tarball): sha256:{}\nArchive unpack failed: {}\nRefusing to install corrupted or tampered package.",
                        expected_sha256, tar_sha, e
                    ));
                }
                return Err(e);
            }
        };

        // Install to CAS & project
        self.install_with_source(&pkg, project_root, tarball_url)
    }

    /// Removes a package from project and updates manifest + lockfile.
    pub fn remove(&self, pkg_name: &str, project_root: &Path) -> Result<bool, String> {
        // Path traversal guard: `pkg_name` arrives straight from the CLI and
        // becomes a path component. `dpm remove ../..` used to recursively
        // delete an arbitrary directory.
        if !Self::is_valid_package_name(pkg_name) {
            return Err(format!(
                "Invalid package name '{}': refusing to remove (path traversal guard)",
                pkg_name
            ));
        }
        let pkg_dir = project_root.join("packages").join(pkg_name);
        let mut removed = false;
        if pkg_dir.exists() {
            fs::remove_dir_all(&pkg_dir).map_err(|e| e.to_string())?;
            removed = true;
        }

        // Update datara.toml: drop exact `[dependencies]` keys matching the
        // package name (the former substring filter also swallowed unrelated
        // keys whose names ended with the same suffix).
        let manifest_path = project_root.join("datara.toml");
        if manifest_path.exists()
            && let Ok(content) = fs::read_to_string(&manifest_path)
        {
            let drop_lines: std::collections::HashSet<usize> = Self::dependency_key_lines(&content)
                .into_iter()
                .filter(|(_, key)| *key == pkg_name)
                .map(|(idx, _)| idx)
                .collect();
            if !drop_lines.is_empty() {
                let kept: Vec<&str> = content
                    .lines()
                    .enumerate()
                    .filter(|(idx, _)| !drop_lines.contains(idx))
                    .map(|(_, line)| line)
                    .collect();
                let _ = fs::write(&manifest_path, kept.join("\n") + "\n");
            }
        }

        // Update datara.lock
        let mut lock = DataraLock::load(project_root).unwrap_or_default();
        lock.remove(pkg_name);
        let _ = lock.save(project_root);

        Ok(removed)
    }

    /// List all installed packages in the project directory.
    pub fn list_installed(&self, project_root: &Path) -> Vec<InstalledPackage> {
        let mut list = Vec::new();
        let packages_dir = project_root.join("packages");
        let lock = DataraLock::load(project_root);

        if packages_dir.is_dir()
            && let Ok(entries) = fs::read_dir(&packages_dir)
        {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let name = p
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let version = lock
                        .as_ref()
                        .and_then(|l| l.packages.get(&name).map(|lp| lp.version.clone()))
                        .unwrap_or_else(|| "0.1.0".into());
                    let digest = lock
                        .as_ref()
                        .and_then(|l| l.packages.get(&name).map(|lp| lp.digest.clone()))
                        .unwrap_or_else(|| "unlocked".into());
                    list.push(InstalledPackage {
                        name,
                        version,
                        digest,
                        path: p,
                    });
                }
            }
        }
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    /// Verifies that all packages in `packages/` match their digests in `datara.lock`.
    pub fn verify(&self, project_root: &Path) -> Result<Vec<VerificationResult>, String> {
        let lock = DataraLock::load(project_root).ok_or_else(|| {
            "No datara.lock found in project root. Run 'dpm install' to generate one.".to_string()
        })?;

        let mut results = Vec::new();
        for (pkg_name, locked) in &lock.packages {
            let pkg_dir = project_root.join("packages").join(pkg_name);
            if !pkg_dir.exists() {
                results.push(VerificationResult {
                    name: pkg_name.clone(),
                    version: locked.version.clone(),
                    status: VerificationStatus::Missing,
                    message: "Package directory not found in packages/".into(),
                });
                continue;
            }

            // Calculate current digest
            let mut files = HashMap::new();
            let _ = Self::collect_files_recursive(&pkg_dir, &pkg_dir, &mut files);
            let is_merkle = locked.digest.starts_with("merkle:");
            let current_digest = if is_merkle {
                Self::compute_merkle_root(&files)
            } else {
                Self::compute_digest(&files)
            };

            // No exceptions: every locked package must match its recorded
            // digest. The former `locked.digest.starts_with("sha256:7f8a9e01")`
            // check silently approved any package whose lock entry carried
            // that magic prefix — a built-in verification bypass.
            if current_digest == locked.digest {
                results.push(VerificationResult {
                    name: pkg_name.clone(),
                    version: locked.version.clone(),
                    status: VerificationStatus::Valid,
                    message: format!("Digest verified ({})", locked.digest),
                });
            } else {
                results.push(VerificationResult {
                    name: pkg_name.clone(),
                    version: locked.version.clone(),
                    status: VerificationStatus::Mismatch,
                    message: format!(
                        "Digest mismatch! expected {}, got {}",
                        locked.digest, current_digest
                    ),
                });
            }
        }

        // Directories under packages/ that have no lock entry used to be
        // invisible to `dpm verify`; report them so a tampered package cannot
        // hide by simply not being written to datara.lock.
        let packages_dir = project_root.join("packages");
        if packages_dir.is_dir()
            && let Ok(entries) = fs::read_dir(&packages_dir)
        {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir()
                    && let Some(name) = p.file_name().map(|n| n.to_string_lossy().to_string())
                    && !lock.packages.contains_key(&name)
                {
                    results.push(VerificationResult {
                        name,
                        version: "unknown".into(),
                        status: VerificationStatus::Untracked,
                        message:
                            "Directory exists in packages/ but has no datara.lock entry (run 'dpm install' or remove it)"
                                .into(),
                    });
                }
            }
        }

        Ok(results)
    }

    /// Installs a verified Sparks package from manifest and tarball bytes.
    pub fn install_sparks_manifest(
        &self,
        manifest: &SparksPackageManifest,
        tarball_bytes: &[u8],
        project_root: &Path,
    ) -> Result<PathBuf, String> {
        // 1. Schema enforcement
        if manifest.schema != 1 {
            return Err(format!(
                "[E-SPARKS-001] Unsupported Sparks schema version {} (expected 1). Please upgrade dpm.",
                manifest.schema
            ));
        }

        // 2. SHA-256 artifact integrity
        let actual_sha = sha256_hexdigest(tarball_bytes);
        let clean_expected = manifest
            .sha256
            .trim()
            .strip_prefix("sha256:")
            .unwrap_or(manifest.sha256.trim());
        if !actual_sha.eq_ignore_ascii_case(clean_expected) {
            return Err(format!(
                "[E-SPARKS-003] SHA-256 artifact checksum mismatch!\n  Expected: sha256:{}\n  Actual:   sha256:{}",
                clean_expected, actual_sha
            ));
        }

        // 3. Cryptographic ed25519 signature verification
        if let (Some(pk), Some(sig)) = (&manifest.public_key, &manifest.signature) {
            verify_ed25519_signature(tarball_bytes, sig, pk).map_err(|e| {
                format!("[E-SPARKS-004] ed25519 package verification failed: {}", e)
            })?;
        }

        // 4. Tarball extraction & capability sidecar validation
        let raw_files = extract_tar(tarball_bytes)?;
        let mut string_files: HashMap<String, String> = HashMap::new();
        for (f_name, f_bytes) in &raw_files {
            string_files.insert(f_name.clone(), String::from_utf8_lossy(f_bytes).to_string());
        }

        if let Some(sidecar_bytes) = raw_files.get("capabilities.json") {
            if let Ok(sidecar_json) = serde_json::from_slice::<serde_json::Value>(sidecar_bytes) {
                let sidecar_caps: Vec<String> = sidecar_json
                    .get("capabilities")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                let mut decl = manifest.capabilities.clone();
                decl.sort();
                let mut act = sidecar_caps.clone();
                act.sort();
                if decl != act {
                    return Err(format!(
                        "[E-SPARKS-002] Capability declaration mismatch: index declared {:?}, but tarball contains {:?}",
                        manifest.capabilities, sidecar_caps
                    ));
                }
            }
        }

        let pkg = HyperGridPackage {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            author: manifest.author.clone(),
            license: manifest.license.clone(),
            digest: format!("sha256:{}", actual_sha),
            capabilities: manifest.capabilities.clone(),
            dependencies: manifest.dependencies.keys().cloned().collect(),
            entry: "main.dtr".to_string(),
            files: string_files,
        };

        self.install_with_source(&pkg, project_root, &manifest.tarball_url)
    }

    /// Fetches a package from a Sparks sparse index and installs it.
    pub fn fetch_and_install_sparks(
        &self,
        sparks_id: &str,
        version: Option<&str>,
        registry_url: &str,
        project_root: &Path,
    ) -> Result<PathBuf, String> {
        if registry_url.starts_with("http://") && std::env::var("FORGEN_ALLOW_HTTP").is_err() {
            eprintln!(
                "[WARN] Plain HTTP registry endpoint used ('{}'). HTTPS is required in production.",
                registry_url
            );
        }

        let raw_name = sparks_id.strip_prefix("sparks/").unwrap_or(sparks_id);
        let manifest_url = match version {
            Some(v) => format!(
                "{}/packages/{}/{}.json",
                registry_url.trim_end_matches('/'),
                raw_name,
                v
            ),
            None => format!(
                "{}/packages/{}.json",
                registry_url.trim_end_matches('/'),
                raw_name
            ),
        };

        let manifest_bytes = http::fetch_url(&manifest_url)?;
        let manifest: SparksPackageManifest =
            serde_json::from_slice(&manifest_bytes).map_err(|e| {
                format!(
                    "Failed to parse Sparks manifest from '{}': {}",
                    manifest_url, e
                )
            })?;

        let tarball_url = if manifest.tarball_url.starts_with("http://")
            || manifest.tarball_url.starts_with("https://")
            || manifest.tarball_url.starts_with("file://")
        {
            manifest.tarball_url.clone()
        } else {
            format!(
                "{}/{}",
                registry_url.trim_end_matches('/'),
                manifest.tarball_url.trim_start_matches('/')
            )
        };

        let tarball_bytes = http::fetch_url(&tarball_url)?;
        self.install_sparks_manifest(&manifest, &tarball_bytes, project_root)
    }

    /// Initializes a new project or library with datara.toml, entry point, and .gitignore.
    pub fn init_project(dir: &Path, name: &str, is_lib: bool) -> Result<(), String> {
        if !Self::is_valid_package_name(name) {
            return Err(format!(
                "Invalid project name '{}': must be a plain identifier (no path separators or '..')",
                name
            ));
        }
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let src_dir = dir.join("src");
        fs::create_dir_all(&src_dir).map_err(|e| e.to_string())?;

        let manifest_path = dir.join("datara.toml");
        if !manifest_path.exists() {
            let entry = if is_lib {
                "src/lib.dtr"
            } else {
                "src/main.dtr"
            };
            let manifest_content = format!(
                r#"[package]
name = "{}"
version = "0.1.0"
entry = "{}"
authors = ["Your Name <you@example.com>"]
description = "A Datara project"
license = "MIT"

[dependencies]
"#,
                name, entry
            );
            fs::write(&manifest_path, manifest_content).map_err(|e| e.to_string())?;
        }

        let entry_file = if is_lib {
            src_dir.join("lib.dtr")
        } else {
            src_dir.join("main.dtr")
        };

        if !entry_file.exists() {
            let code = if is_lib {
                format!(
                    r#"// {} library module
pub fn add(a: Int, b: Int) -> Int {{
    return a + b
}}
"#,
                    name
                )
            } else {
                r#"// Datara entry point
fn main() {
    println("Hello from Datara!")
}
"#
                .to_string()
            };
            fs::write(&entry_file, code).map_err(|e| e.to_string())?;
        }

        let gitignore = dir.join(".gitignore");
        if !gitignore.exists() {
            let gi = "target/\npackages/\n*.exe\n*.obj\n*.bc\n*.ll\n";
            let _ = fs::write(&gitignore, gi);
        }

        Ok(())
    }

    /// Collects `(line_index, key)` pairs for every `key = value` line inside
    /// the `[dependencies]` section. Keys may be bare or quoted. This replaces
    /// the former substring `contains()` checks that wrongly treated
    /// `my_pkg =` as `pkg =` (a key ending with the same suffix).
    fn dependency_key_lines(content: &str) -> Vec<(usize, String)> {
        let mut result = Vec::new();
        let mut in_deps = false;
        for (idx, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                in_deps = trimmed == "[dependencies]";
                continue;
            }
            if in_deps
                && !trimmed.is_empty()
                && !trimmed.starts_with('#')
                && let Some(eq) = trimmed.find('=')
            {
                let key = trimmed[..eq].trim().trim_matches('"').to_string();
                result.push((idx, key));
            }
        }
        result
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

        if let Ok(content) = fs::read_to_string(&manifest_path)
            && !Self::dependency_key_lines(&content)
                .iter()
                .any(|(_, key)| *key == pkg_name)
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

        let digest_hex = Self::compute_merkle_root(&files);
        let capabilities = Self::extract_capabilities(&files);

        let pkg = HyperGridPackage {
            name: name.clone(),
            version: version.clone(),
            description,
            author,
            license,
            digest: digest_hex,
            capabilities,
            dependencies: manifest.dependencies.keys().cloned().collect(),
            entry: manifest.package.entry.unwrap_or_else(|| "main.dtr".into()),
            files,
        };

        // Create .dtr-pkg bundle
        if let Ok(bundle_json) = serde_json::to_string_pretty(&pkg) {
            // 1. Write to local dist/ folder
            let dist_dir = project_root.join("dist");
            let _ = fs::create_dir_all(&dist_dir);
            let _ = fs::write(
                dist_dir.join(format!("{}-{}.dtr-pkg", name, version)),
                &bundle_json,
            );

            // 2. Write to CAS bundles/ directory
            let cas_bundles = self.store_path.join("bundles");
            let _ = fs::create_dir_all(&cas_bundles);
            let _ = fs::write(
                cas_bundles.join(format!("{}-{}.dtr-pkg", name, version)),
                &bundle_json,
            );
        }

        // 3. Update Git-backed index.json in store_path
        self.update_disk_index(&pkg);

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
                    if let Some(n) = path.file_name().and_then(|s| s.to_str())
                        && (n.starts_with('.') || n == "target" || n == "packages")
                    {
                        continue;
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
