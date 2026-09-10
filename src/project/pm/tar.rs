//! POSIX ustar tar archive creation and extraction with strict path traversal
//! (zip-slip) security checks.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const BLOCK_SIZE: usize = 512;

/// Sanitizes relative paths inside archives to prevent zip-slip / directory traversal.
pub fn sanitize_tar_path(rel: &str) -> Result<String, String> {
    let normalized = rel.replace('\\', "/");
    let trimmed = normalized.trim_start_matches("./");

    if trimmed.is_empty() {
        return Err("Archive entry has an empty filename".to_string());
    }
    if trimmed.starts_with('/') {
        return Err(format!("Absolute path forbidden in archive: '{}'", rel));
    }
    if trimmed.contains('\0') {
        return Err(format!("NUL byte detected in archive filename: '{}'", rel));
    }
    if trimmed.contains(':') {
        return Err(format!(
            "Drive letter or stream syntax forbidden in archive: '{}'",
            rel
        ));
    }
    for component in trimmed.split('/') {
        if component == ".." {
            return Err(format!(
                "Path traversal ('..') detected and rejected in archive: '{}'",
                rel
            ));
        }
    }
    Ok(trimmed.to_string())
}

/// Computes the 512-byte ustar header checksum.
fn compute_header_checksum(header: &[u8; BLOCK_SIZE]) -> u32 {
    let mut sum: u32 = 0;
    for (i, &b) in header.iter().enumerate() {
        if (148..156).contains(&i) {
            sum += 32; // treat checksum field as ASCII spaces
        } else {
            sum += b as u32;
        }
    }
    sum
}

/// Creates a POSIX ustar tar archive from a map of relative file paths to file bytes.
pub fn create_tar(files: &HashMap<String, Vec<u8>>) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut sorted_keys: Vec<&String> = files.keys().collect();
    sorted_keys.sort();

    for rel_path in sorted_keys {
        let safe_name = sanitize_tar_path(rel_path)?;
        let data = files.get(rel_path).unwrap();

        if safe_name.len() > 100 {
            return Err(format!(
                "Filename '{}' exceeds 100-byte ustar limit without long-prefix",
                safe_name
            ));
        }

        let mut header = [0u8; BLOCK_SIZE];

        // 0..100: name
        let name_bytes = safe_name.as_bytes();
        header[0..name_bytes.len()].copy_from_slice(name_bytes);

        // 100..108: mode (0000644\0)
        header[100..108].copy_from_slice(b"0000644\0");

        // 108..116: uid (0000000\0)
        header[108..116].copy_from_slice(b"0000000\0");

        // 116..124: gid (0000000\0)
        header[116..124].copy_from_slice(b"0000000\0");

        // 124..136: size (11 octal digits + space)
        let size_str = format!("{:011o} ", data.len());
        header[124..136].copy_from_slice(size_str.as_bytes());

        // 136..148: mtime (11 octal digits + space)
        header[136..148].copy_from_slice(b"00000000000 ");

        // 156: typeflag ('0' = regular file)
        header[156] = b'0';

        // 257..263: magic ("ustar\0")
        header[257..263].copy_from_slice(b"ustar\0");

        // 263..265: version ("00")
        header[263..265].copy_from_slice(b"00");

        // Compute checksum
        let chksum = compute_header_checksum(&header);
        let chksum_str = format!("{:06o}\0 ", chksum);
        header[148..156].copy_from_slice(chksum_str.as_bytes());

        out.extend_from_slice(&header);
        out.extend_from_slice(data);

        // Pad to next 512-byte boundary
        let remainder = data.len() % BLOCK_SIZE;
        if remainder != 0 {
            let padding = BLOCK_SIZE - remainder;
            out.extend(std::iter::repeat(0).take(padding));
        }
    }

    // Two 512-byte blocks of zeros mark end of archive
    out.extend(std::iter::repeat(0).take(BLOCK_SIZE * 2));
    Ok(out)
}

/// Extracts all files from a POSIX ustar tar archive in memory into a map of path -> bytes.
pub fn extract_tar(data: &[u8]) -> Result<HashMap<String, Vec<u8>>, String> {
    let mut files = HashMap::new();
    let mut offset = 0;

    while offset + BLOCK_SIZE <= data.len() {
        let block = &data[offset..offset + BLOCK_SIZE];

        // Check for end of archive (all zeros)
        if block.iter().all(|&b| b == 0) {
            break;
        }

        let mut header = [0u8; BLOCK_SIZE];
        header.copy_from_slice(block);

        // Verify checksum
        let chksum_str = std::str::from_utf8(&header[148..156])
            .map_err(|e| format!("Invalid UTF-8 in tar checksum: {}", e))?
            .trim()
            .trim_matches('\0');
        let expected_chksum = u32::from_str_radix(chksum_str, 8)
            .map_err(|e| format!("Failed to parse tar checksum '{}': {}", chksum_str, e))?;

        let actual_chksum = compute_header_checksum(&header);
        if expected_chksum != actual_chksum {
            return Err(format!(
                "Tar header checksum mismatch at offset {}: expected {:o}, got {:o}",
                offset, expected_chksum, actual_chksum
            ));
        }

        // Filename
        let name_bytes = &header[0..100];
        let name_end = name_bytes.iter().position(|&b| b == 0).unwrap_or(100);
        let raw_name = std::str::from_utf8(&name_bytes[..name_end])
            .map_err(|e| format!("Invalid UTF-8 in tar filename: {}", e))?;

        // Optional prefix (bytes 345..500)
        let prefix_bytes = &header[345..500];
        let prefix_end = prefix_bytes.iter().position(|&b| b == 0).unwrap_or(155);
        let raw_prefix = std::str::from_utf8(&prefix_bytes[..prefix_end]).unwrap_or("");

        let full_name = if !raw_prefix.is_empty() {
            format!("{}/{}", raw_prefix, raw_name)
        } else {
            raw_name.to_string()
        };

        let safe_name = sanitize_tar_path(&full_name)?;

        // Size
        let size_str = std::str::from_utf8(&header[124..136])
            .map_err(|e| format!("Invalid UTF-8 in tar size field: {}", e))?
            .trim()
            .trim_matches('\0');
        let size = usize::from_str_radix(size_str, 8)
            .map_err(|e| format!("Failed to parse tar file size '{}': {}", size_str, e))?;

        let typeflag = header[156];
        offset += BLOCK_SIZE;

        let end = match offset.checked_add(size) {
            Some(e) if e <= data.len() => e,
            _ => {
                return Err(format!(
                    "Tar file '{}' claims size {} exceeding archive length",
                    safe_name, size
                ));
            }
        };

        if typeflag == b'0' || typeflag == 0 {
            // Regular file
            let file_data = data[offset..end].to_vec();
            files.insert(safe_name, file_data);
        }

        let remainder = size % BLOCK_SIZE;
        let pad = if remainder == 0 {
            0
        } else {
            BLOCK_SIZE - remainder
        };
        offset = match end.checked_add(pad) {
            Some(o) => o,
            None => return Err("Archive offset overflow".into()),
        };
    }

    Ok(files)
}

/// Extracts a tar archive safely to a target directory on disk.
pub fn extract_tar_to_dir(data: &[u8], dest_dir: &Path) -> Result<Vec<PathBuf>, String> {
    fs::create_dir_all(dest_dir).map_err(|e| format!("Failed to create destination dir: {}", e))?;
    let files = extract_tar(data)?;
    let mut written = Vec::new();

    for (rel_path, content) in files {
        let safe_rel = sanitize_tar_path(&rel_path)?;
        let target_path = dest_dir.join(&safe_rel);
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create parent directory for '{}': {}",
                    safe_rel, e
                )
            })?;
        }
        fs::write(&target_path, content).map_err(|e| {
            format!(
                "Failed to write extracted file '{}': {}",
                target_path.display(),
                e
            )
        })?;
        written.push(target_path);
    }

    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tar_roundtrip() {
        let mut original = HashMap::new();
        original.insert("main.dtr".to_string(), b"fn main() { out 42 }".to_vec());
        original.insert(
            "lib/math.dtr".to_string(),
            b"pub fn add(a: Int, b: Int) -> Int => a + b".to_vec(),
        );

        let tar_bytes = create_tar(&original).expect("Archive creation must succeed");
        assert!(!tar_bytes.is_empty());
        assert_eq!(tar_bytes.len() % 512, 0);

        let extracted = extract_tar(&tar_bytes).expect("Archive extraction must succeed");
        assert_eq!(extracted.len(), 2);
        assert_eq!(extracted.get("main.dtr").unwrap(), b"fn main() { out 42 }");
        assert_eq!(
            extracted.get("lib/math.dtr").unwrap(),
            b"pub fn add(a: Int, b: Int) -> Int => a + b"
        );
    }

    #[test]
    fn test_tar_zip_slip_rejection() {
        assert!(sanitize_tar_path("../evil.dtr").is_err());
        assert!(sanitize_tar_path("foo/../../evil.dtr").is_err());
        assert!(sanitize_tar_path("/etc/passwd").is_err());
        assert!(sanitize_tar_path("C:\\windows\\system32").is_err());
    }

    #[test]
    fn test_tar_size_overflow_rejection() {
        // A hostile archive claiming a size close to usize::MAX must not panic with arithmetic overflow.
        let mut header = [0u8; BLOCK_SIZE];
        header[0..4].copy_from_slice(b"test");
        header[100..108].copy_from_slice(b"0000644\0");
        header[124..136].copy_from_slice(b"77777777777 "); // Large octal size
        header[156] = b'0';
        let chksum = compute_header_checksum(&header);
        let chksum_str = format!("{:06o}\0 ", chksum);
        header[148..156].copy_from_slice(chksum_str.as_bytes());

        let mut archive = header.to_vec();
        archive.extend_from_slice(&[0u8; BLOCK_SIZE * 2]);
        assert!(extract_tar(&archive).is_err());
    }
}
