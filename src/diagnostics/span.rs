use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SourceSpan {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub file: String,
}

impl SourceSpan {
    pub fn new(
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
        file: String,
    ) -> Self {
        Self {
            start_line,
            start_col,
            end_line,
            end_col,
            file,
        }
    }

    pub fn point(line: usize, col: usize, file: String) -> Self {
        Self {
            start_line: line,
            start_col: col,
            end_line: line,
            end_col: col,
            file,
        }
    }
}

impl std::fmt::Display for SourceSpan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.start_line, self.start_col)
    }
}

pub fn is_same_file_or_module(file_a: &str, file_b: &str) -> bool {
    if file_a.is_empty() || file_b.is_empty() {
        return true;
    }
    if file_a == file_b {
        return true;
    }
    let norm_a = file_a.replace('\\', "/");
    let norm_b = file_b.replace('\\', "/");
    let trim_a = norm_a.trim_start_matches("./");
    let trim_b = norm_b.trim_start_matches("./");
    if trim_a == trim_b {
        return true;
    }
    if let (Ok(p_a), Ok(p_b)) = (std::fs::canonicalize(file_a), std::fs::canonicalize(file_b)) {
        return p_a == p_b;
    }
    false
}
