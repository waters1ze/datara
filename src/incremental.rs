use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleFingerprint {
    pub path: String,
    pub hash: String,
    pub last_modified: u64,
    pub direct_dependencies: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IncrementalCache {
    pub fingerprints: HashMap<String, ModuleFingerprint>,
}

impl IncrementalCache {
    pub fn new() -> Self {
        Self {
            fingerprints: HashMap::new(),
        }
    }

    pub fn load_from_dir(cache_dir: &Path) -> Self {
        let cache_file = cache_dir.join("incremental.json");
        if let Ok(content) = fs::read_to_string(&cache_file) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Self::new()
        }
    }

    /// Persists the cache atomically: the JSON is written to a temporary file
    /// first and then moved into place. The former implementation wrote
    /// directly into `incremental.json`, so a crash or power loss mid-write
    /// corrupted the cache (the same failure mode `profile.dev` already saw
    /// with rustc's own incremental artifacts).
    pub fn save_to_dir(&self, cache_dir: &Path) -> Result<(), String> {
        fs::create_dir_all(cache_dir).map_err(|e| e.to_string())?;
        let cache_file = cache_dir.join("incremental.json");
        let tmp_file = cache_dir.join("incremental.json.tmp");
        let content = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&tmp_file, content).map_err(|e| e.to_string())?;
        // std::fs::rename fails on Windows when the destination exists.
        if cache_file.exists() {
            let _ = fs::remove_file(&cache_file);
        }
        fs::rename(&tmp_file, &cache_file).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Deterministic lightweight 64-bit FNV-1a hash.
    ///
    /// This is a *cache* hash, not a cryptographic one: it guards against
    /// accidental staleness, not against adversarial content.
    pub fn calculate_hash(content: &str) -> String {
        // Deterministic lightweight 64-bit FNV-1a hash
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in content.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        format!("{:016x}", hash)
    }

    pub fn is_module_fresh(&self, path: &Path, content: &str) -> bool {
        let path_str = path.to_string_lossy().to_string();
        if let Some(fp) = self.fingerprints.get(&path_str) {
            let current_hash = Self::calculate_hash(content);
            fp.hash == current_hash
        } else {
            false
        }
    }

    /// Freshness including the transitive dependency closure: the module's own
    /// hash must match, and every recorded dependency must itself be fresh.
    /// Dependencies that cannot be resolved to a file are skipped (the cache
    /// currently records dependency names opportunistically), so this is a
    /// strictly stronger check than [`Self::is_module_fresh`].
    pub fn is_module_fresh_transitive(&self, path: &Path, content: &str) -> bool {
        let mut visited = HashSet::new();
        self.check_transitive(path, content, &mut visited)
    }

    fn check_transitive(&self, path: &Path, content: &str, visited: &mut HashSet<String>) -> bool {
        let path_str = path.to_string_lossy().to_string();
        if !visited.insert(path_str.clone()) {
            return true; // already verified on this walk
        }
        let Some(fp) = self.fingerprints.get(&path_str) else {
            return false;
        };
        if fp.hash != Self::calculate_hash(content) {
            return false;
        }
        for dep in &fp.direct_dependencies {
            let Some(dep_path) = self.resolve_dependency(path, dep) else {
                continue;
            };
            let Ok(dep_content) = fs::read_to_string(&dep_path) else {
                return false; // recorded dependency disappeared -> stale
            };
            if !self.check_transitive(&dep_path, &dep_content, visited) {
                return false;
            }
        }
        true
    }

    /// Resolves a recorded dependency (a bare module name like `"b"` or a
    /// path) to an existing source file. Returns `None` when nothing matches.
    fn resolve_dependency(&self, from: &Path, dep: &str) -> Option<PathBuf> {
        if dep.is_empty() {
            return None;
        }
        let dep_path = Path::new(dep);
        if dep_path.is_absolute() && dep_path.exists() {
            return Some(dep_path.to_path_buf());
        }
        let base = from.parent().unwrap_or_else(|| Path::new("."));
        let candidates = [
            base.join(format!("{}.dtr", dep)),
            base.join(dep),
            base.join("src").join(format!("{}.dtr", dep)),
        ];
        for cand in candidates {
            if cand.exists() {
                return Some(cand);
            }
        }
        // Fall back to matching cache keys by file stem / file name.
        self.fingerprints.keys().find_map(|key| {
            let p = Path::new(key);
            let matches = p.file_stem().and_then(|s| s.to_str()) == Some(dep)
                || p.file_name().and_then(|s| s.to_str()) == Some(dep);
            matches.then(|| p.to_path_buf())
        })
    }

    pub fn update_module(&mut self, path: &Path, content: &str, dependencies: Vec<String>) {
        let path_str = path.to_string_lossy().to_string();
        let hash = Self::calculate_hash(content);
        self.fingerprints.insert(
            path_str.clone(),
            ModuleFingerprint {
                path: path_str,
                hash,
                last_modified: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                direct_dependencies: dependencies,
            },
        );
    }

    pub fn invalidate_module(&mut self, path: &Path) {
        let path_str = path.to_string_lossy().to_string();
        self.fingerprints.remove(&path_str);
    }
}
