use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleFingerprint {
    pub path: String,
    pub hash: String,
    pub source_hash: String,
    pub interface_hash: String,
    pub ir_hash: String,
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

    pub fn save_to_dir(&self, cache_dir: &Path) -> Result<(), String> {
        let _ = fs::create_dir_all(cache_dir);
        let cache_file = cache_dir.join("incremental.json");
        let content = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(cache_file, content).map_err(|e| e.to_string())?;
        Ok(())
    }

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

    pub fn is_interface_fresh(&self, path: &Path, interface_content: &str) -> bool {
        let path_str = path.to_string_lossy().to_string();
        if let Some(fp) = self.fingerprints.get(&path_str) {
            let current_hash = Self::calculate_hash(interface_content);
            fp.interface_hash == current_hash
        } else {
            false
        }
    }

    pub fn update_module(&mut self, path: &Path, content: &str, dependencies: Vec<String>) {
        let path_str = path.to_string_lossy().to_string();
        let hash = Self::calculate_hash(content);
        self.fingerprints.insert(
            path_str.clone(),
            ModuleFingerprint {
                path: path_str,
                hash: hash.clone(),
                source_hash: hash.clone(),
                interface_hash: hash,
                ir_hash: String::new(),
                last_modified: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                direct_dependencies: dependencies,
            },
        );
    }

    pub fn update_module_layered(
        &mut self,
        path: &Path,
        source: &str,
        interface: &str,
        ir: &str,
        dependencies: Vec<String>,
    ) {
        let path_str = path.to_string_lossy().to_string();
        let s_hash = Self::calculate_hash(source);
        let if_hash = Self::calculate_hash(interface);
        let ir_h = Self::calculate_hash(ir);
        self.fingerprints.insert(
            path_str.clone(),
            ModuleFingerprint {
                path: path_str,
                hash: s_hash.clone(),
                source_hash: s_hash,
                interface_hash: if_hash,
                ir_hash: ir_h,
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
