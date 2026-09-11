use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
    pub digest: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationStatus {
    Valid,
    Mismatch,
    Missing,
    /// Directory present in packages/ but absent from datara.lock.
    Untracked,
}

#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub name: String,
    pub version: String,
    pub status: VerificationStatus,
    pub message: String,
}
