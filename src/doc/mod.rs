//! Autonomous Single-File SPA Documentation Generator for Datara (`forgen doc`)

use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct DocItem {
    pub name: String,
    pub kind: String, // "fn", "class", "behavior"
    pub signature: String,
    pub doc_comment: String,
    pub file: String,
    pub effects: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DocModule {
    pub name: String,
    pub path: String,
    pub items: Vec<DocItem>,
}

/// Generates standalone HTML documentation from Datara source files
pub fn generate_docs(target_dir: &Path, output_file: &Path) -> Result<usize, String> {
    let mut modules = Vec::new();
    collect_and_parse_docs(target_dir, &mut modules)?;

    if modules.is_empty() {
        return Err("No Datara (.dtr or .forge) files found to document.".to_string());
    }

    let mut total_items = 0;
    for m in &modules {
        total_items += m.items.len();
    }

    let html = render_spa_html(&modules);
    if let Some(parent) = output_file.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(output_file, html).map_err(|e| e.to_string())?;

    Ok(total_items)
}

fn collect_and_parse_docs(dir: &Path, modules: &mut Vec<DocModule>) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    if dir.is_file() {
        if let Some(ext) = dir.extension().and_then(|s| s.to_str())
            && matches!(ext, "dtr" | "forge")
        {
            let mod_name = dir.file_stem().and_then(|s| s.to_str()).unwrap_or("module");
            let content = fs::read_to_string(dir).unwrap_or_default();
            let items = parse_file_doc_items(&content, &dir.to_string_lossy());
            modules.push(DocModule {
                name: mod_name.to_string(),
                path: dir.to_string_lossy().to_string(),
                items,
            });
        }
        return Ok(());
    }

    let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let dirname = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if dirname == "target" || dirname == ".git" || dirname == "packages" {
                continue;
            }
            collect_and_parse_docs(&path, modules)?;
        } else if let Some(ext) = path.extension().and_then(|s| s.to_str())
            && matches!(ext, "dtr" | "forge")
        {
            let mod_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("module");
            let content = fs::read_to_string(&path).unwrap_or_default();
            let items = parse_file_doc_items(&content, &path.to_string_lossy());
            modules.push(DocModule {
                name: mod_name.to_string(),
                path: path.to_string_lossy().to_string(),
                items,
            });
        }
    }
    Ok(())
}

fn parse_file_doc_items(content: &str, file_path: &str) -> Vec<DocItem> {
    let mut items = Vec::new();
    let mut pending_doc = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("///") {
            pending_doc.push(trimmed.trim_start_matches("///").trim().to_string());
            continue;
        }

        if trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ") {
            let doc_text = pending_doc.join(" ");
            pending_doc.clear();

            let sig_end = trimmed.find('{').unwrap_or(trimmed.len());
            let sig = trimmed[..sig_end].trim().to_string();
            let name = sig
                .strip_prefix("pub ")
                .unwrap_or(&sig)
                .strip_prefix("fn ")
                .and_then(|s| s.split('(').next())
                .unwrap_or("unknown")
                .trim()
                .to_string();

            let mut effects = Vec::new();
            if trimmed.contains("[pure]") {
                effects.push("pure".to_string());
            }
            if trimmed.contains("[io]") {
                effects.push("io".to_string());
            }
            if trimmed.contains("[net]") {
                effects.push("net".to_string());
            }
            if trimmed.contains("[mut]") {
                effects.push("mut".to_string());
            }
            if effects.is_empty() {
                effects.push("pure".to_string());
            }

            items.push(DocItem {
                name,
                kind: "fn".to_string(),
                signature: sig,
                doc_comment: doc_text,
                file: file_path.to_string(),
                effects,
            });
        } else if trimmed.starts_with("class ") {
            let doc_text = pending_doc.join(" ");
            pending_doc.clear();

            let sig_end = trimmed.find('{').unwrap_or(trimmed.len());
            let sig = trimmed[..sig_end].trim().to_string();
            let name = sig
                .strip_prefix("class ")
                .and_then(|s| s.split('<').next())
                .unwrap_or("unknown")
                .trim()
                .to_string();

            items.push(DocItem {
                name,
                kind: "class".to_string(),
                signature: sig,
                doc_comment: doc_text,
                file: file_path.to_string(),
                effects: vec!["pure".to_string()],
            });
        } else if !trimmed.is_empty() && !trimmed.starts_with("//") {
            pending_doc.clear();
        }
    }

    items
}

fn render_spa_html(modules: &[DocModule]) -> String {
    let mut modules_json = String::new();
    modules_json.push('[');
    for (i, m) in modules.iter().enumerate() {
        if i > 0 {
            modules_json.push(',');
        }
        modules_json.push_str(&format!(
            r#"{{"name":"{}","path":"{}","items":["#,
            m.name,
            m.path.replace('\\', "/")
        ));
        for (j, item) in m.items.iter().enumerate() {
            if j > 0 {
                modules_json.push(',');
            }
            modules_json.push_str(&format!(
                r#"{{"name":"{}","kind":"{}","sig":"{}","doc":"{}","effects":[{}]}}"#,
                item.name,
                item.kind,
                item.signature.replace('"', "\\\""),
                item.doc_comment.replace('"', "\\\""),
                item.effects
                    .iter()
                    .map(|e| format!("\"{}\"", e))
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
        modules_json.push_str("]}");
    }
    modules_json.push(']');

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Datara API Documentation</title>
    <style>
        :root {{
            --bg-primary: #0d1117;
            --bg-secondary: #161b22;
            --bg-tertiary: #21262d;
            --text-primary: #c9d1d9;
            --text-heading: #f0f6fc;
            --accent-cyan: #38bdf8;
            --accent-green: #4ade80;
            --accent-purple: #c084fc;
            --accent-orange: #fb923c;
            --border-color: #30363d;
        }}
        * {{ box-sizing: border-box; margin: 0; padding: 0; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, monospace; }}
        body {{ background: var(--bg-primary); color: var(--text-primary); display: flex; height: 100vh; overflow: hidden; }}
        #sidebar {{ width: 320px; background: var(--bg-secondary); border-right: 1px solid var(--border-color); display: flex; flex-direction: column; }}
        #header {{ padding: 18px; border-bottom: 1px solid var(--border-color); }}
        #header h1 {{ font-size: 1.25rem; color: var(--text-heading); display: flex; align-items: center; gap: 8px; }}
        #search {{ width: 100%; margin-top: 12px; padding: 8px 12px; background: var(--bg-primary); border: 1px solid var(--border-color); border-radius: 6px; color: var(--text-heading); outline: none; }}
        #search:focus {{ border-color: var(--accent-cyan); }}
        #tree {{ flex: 1; overflow-y: auto; padding: 12px; }}
        .mod-title {{ font-size: 0.8rem; text-transform: uppercase; color: #8b949e; letter-spacing: 0.5px; margin: 12px 0 6px 8px; font-weight: bold; }}
        .nav-item {{ padding: 6px 10px; border-radius: 6px; cursor: pointer; display: flex; align-items: center; justify-content: space-between; font-size: 0.88rem; transition: background 0.15s; }}
        .nav-item:hover {{ background: var(--bg-tertiary); color: var(--text-heading); }}
        .nav-item.active {{ background: #1f6feb22; border-left: 3px solid var(--accent-cyan); color: var(--accent-cyan); font-weight: 600; }}
        #content {{ flex: 1; overflow-y: auto; padding: 40px; }}
        .item-card {{ background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: 8px; padding: 24px; margin-bottom: 24px; }}
        .item-header {{ display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; }}
        .item-title {{ font-size: 1.4rem; color: var(--text-heading); font-weight: 700; }}
        .badge {{ padding: 3px 8px; border-radius: 12px; font-size: 0.75rem; font-weight: bold; text-transform: uppercase; margin-left: 6px; }}
        .badge-pure {{ background: #22c55e22; color: var(--accent-green); border: 1px solid #22c55e44; }}
        .badge-io {{ background: #38bdf822; color: var(--accent-cyan); border: 1px solid #38bdf844; }}
        .badge-net {{ background: #c084fc22; color: var(--accent-purple); border: 1px solid #c084fc44; }}
        .badge-mut {{ background: #fb923c22; color: var(--accent-orange); border: 1px solid #fb923c44; }}
        .signature {{ background: var(--bg-primary); border: 1px solid var(--border-color); border-radius: 6px; padding: 12px; font-family: monospace; font-size: 0.95rem; color: #79c0ff; overflow-x: auto; margin: 12px 0; }}
        .doc-text {{ color: #8b949e; line-height: 1.6; margin-top: 10px; font-size: 0.92rem; }}
    </style>
</head>
<body>
    <div id="sidebar">
        <div id="header">
            <h1>⚡ Datara Docs</h1>
            <input type="text" id="search" placeholder="Search functions, classes, effects..." oninput="filterDocs()">
        </div>
        <div id="tree"></div>
    </div>
    <div id="content"></div>

    <script>
        const modules = {modules_json};

        function renderTree(filter = "") {{
            const tree = document.getElementById("tree");
            tree.innerHTML = "";
            const lower = filter.toLowerCase();

            modules.forEach(mod => {{
                const matching = mod.items.filter(it => it.name.toLowerCase().includes(lower) || it.sig.toLowerCase().includes(lower));
                if (matching.length === 0) return;

                const modHead = document.createElement("div");
                modHead.className = "mod-title";
                modHead.innerText = mod.name;
                tree.appendChild(modHead);

                matching.forEach(it => {{
                    const div = document.createElement("div");
                    div.className = "nav-item";
                    div.innerHTML = `<span>${{it.name}}</span><span style="font-size:0.75rem; color:#8b949e">${{it.kind}}</span>`;
                    div.onclick = () => showItem(it);
                    tree.appendChild(div);
                }});
            }});
        }}

        function showItem(it) {{
            const content = document.getElementById("content");
            const badges = it.effects.map(e => `<span class="badge badge-${{e}}">${{e}}</span>`).join("");
            content.innerHTML = `
                <div class="item-card">
                    <div class="item-header">
                        <div class="item-title">${{it.name}}</div>
                        <div>${{badges}}</div>
                    </div>
                    <div class="signature">${{it.sig}}</div>
                    <div class="doc-text">${{it.doc || "<em>No documentation provided.</em>"}}</div>
                </div>
            `;
        }}

        function filterDocs() {{
            const val = document.getElementById("search").value;
            renderTree(val);
        }}

        renderTree();
        if (modules.length > 0 && modules[0].items.length > 0) {{
            showItem(modules[0].items[0]);
        }}
    </script>
</body>
</html>
"#
    )
}
