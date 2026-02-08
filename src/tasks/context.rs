//! Gather codebase context for an issue or PR
//!
//! Usage:
//!   lok context --issue 123
//!   lok context --pr 456
//!   lok context "search query"

use anyhow::{Context, Result};
use colored::Colorize;
use serde::Deserialize;
use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

/// Regex for extracting file:line references
static FILE_REF_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"([a-zA-Z0-9_/.-]+\.(rs|rb|py|js|ts|go|java|c|cpp|h|hpp|tsx|jsx)):(\d+)")
        .unwrap()
});

#[derive(Debug, Deserialize)]
struct GitHubIssue {
    number: u64,
    title: String,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubPR {
    number: u64,
    title: String,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PRFile {
    path: String,
}

#[derive(Debug)]
struct FileRef {
    path: String,
    line: usize,
}

pub async fn run(
    dir: &Path,
    issue: Option<&str>,
    pr: Option<&str>,
    query: Option<&str>,
    verbose: bool,
) -> Result<()> {
    let mut context = String::new();
    let search_text;

    // Gather source text based on input type
    if let Some(issue_ref) = issue {
        let issue_num = parse_ref(issue_ref)?;
        if verbose {
            eprintln!(
                "{} Fetching issue #{}...",
                "context:".cyan().bold(),
                issue_num
            );
        }
        let issue_data = fetch_issue(dir, issue_num)?;
        search_text = format!(
            "{} {}",
            issue_data.title,
            issue_data.body.unwrap_or_default()
        );

        context.push_str(&format!(
            "# Context for Issue #{}: {}\n\n",
            issue_data.number, issue_data.title
        ));
    } else if let Some(pr_ref) = pr {
        let pr_num = parse_ref(pr_ref)?;
        if verbose {
            eprintln!("{} Fetching PR #{}...", "context:".cyan().bold(), pr_num);
        }
        let pr_data = fetch_pr(dir, pr_num)?;
        search_text = format!("{} {}", pr_data.title, pr_data.body.unwrap_or_default());

        context.push_str(&format!(
            "# Context for PR #{}: {}\n\n",
            pr_data.number, pr_data.title
        ));

        // For PRs, also get the changed files
        let changed_files = fetch_pr_files(dir, pr_num)?;
        if !changed_files.is_empty() {
            context.push_str("## Changed Files in PR\n\n");
            for file in &changed_files {
                context.push_str(&format!("- {}\n", file.path));
            }
            context.push('\n');

            // Read contents of changed files (limit to first 10)
            context.push_str("## File Contents\n\n");
            for file in changed_files.iter().take(10) {
                if let Ok(content) = read_file_with_limit(dir, &file.path, 200).await {
                    context.push_str(&format!("### {}\n```\n{}\n```\n\n", file.path, content));
                }
            }
        }
    } else if let Some(q) = query {
        search_text = q.to_string();
        context.push_str(&format!("# Context for: {}\n\n", q));
    } else {
        anyhow::bail!("Provide --issue, --pr, or a search query");
    }

    // Extract file references from the text
    let file_refs = extract_file_references(&search_text);
    if !file_refs.is_empty() {
        context.push_str("## Referenced Files\n\n");
        for file_ref in &file_refs {
            if let Ok(content) = read_file_around_line(dir, &file_ref.path, file_ref.line, 15).await
            {
                context.push_str(&format!(
                    "### {} (line {})\n```\n{}\n```\n\n",
                    file_ref.path, file_ref.line, content
                ));
            }
        }
    }

    // Extract keywords and search codebase
    let keywords = extract_keywords(&search_text);
    if !keywords.is_empty() {
        if verbose {
            eprintln!(
                "{} Searching for: {}",
                "context:".cyan().bold(),
                keywords.join(", ")
            );
        }

        context.push_str("## Keyword Search Results\n\n");
        for keyword in keywords.iter().take(5) {
            if let Ok(results) = grep_codebase(dir, keyword) {
                if !results.is_empty() {
                    context.push_str(&format!(
                        "### Matches for '{}'\n```\n{}\n```\n\n",
                        keyword, results
                    ));
                }
            }
        }
    }

    // Add project structure info
    context.push_str("## Project Structure\n\n");
    if let Ok(structure) = get_project_structure(dir) {
        context.push_str(&format!("```\n{}\n```\n\n", structure));
    }

    // Output the context
    println!("{}", context);

    Ok(())
}

fn parse_ref(reference: &str) -> Result<u64> {
    let trimmed = reference.trim().trim_start_matches('#');

    // Try as number
    if let Ok(num) = trimmed.parse::<u64>() {
        return Ok(num);
    }

    // Try extracting from URL
    if trimmed.contains("/issues/") {
        if let Some(num_str) = trimmed.split("/issues/").last() {
            if let Ok(num) = num_str.trim_end_matches('/').parse::<u64>() {
                return Ok(num);
            }
        }
    }
    if trimmed.contains("/pull/") {
        if let Some(num_str) = trimmed.split("/pull/").last() {
            if let Ok(num) = num_str.trim_end_matches('/').parse::<u64>() {
                return Ok(num);
            }
        }
    }

    anyhow::bail!("Invalid reference: '{}'. Use number or URL.", reference)
}

fn fetch_issue(dir: &Path, number: u64) -> Result<GitHubIssue> {
    let output = Command::new("gh")
        .args([
            "issue",
            "view",
            &number.to_string(),
            "--json",
            "number,title,body",
        ])
        .current_dir(dir)
        .output()
        .context("Failed to run gh command")?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to fetch issue #{}: {}", number, err.trim());
    }

    serde_json::from_slice(&output.stdout).context("Failed to parse issue JSON")
}

fn fetch_pr(dir: &Path, number: u64) -> Result<GitHubPR> {
    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            &number.to_string(),
            "--json",
            "number,title,body,changedFiles",
        ])
        .current_dir(dir)
        .output()
        .context("Failed to run gh command")?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to fetch PR #{}: {}", number, err.trim());
    }

    serde_json::from_slice(&output.stdout).context("Failed to parse PR JSON")
}

fn fetch_pr_files(dir: &Path, number: u64) -> Result<Vec<PRFile>> {
    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            &number.to_string(),
            "--json",
            "files",
            "--jq",
            ".files",
        ])
        .current_dir(dir)
        .output()
        .context("Failed to run gh command")?;

    if !output.status.success() {
        return Ok(vec![]);
    }

    Ok(serde_json::from_slice(&output.stdout).unwrap_or_default())
}

fn extract_file_references(text: &str) -> Vec<FileRef> {
    let mut refs = Vec::new();

    for cap in FILE_REF_RE.captures_iter(text) {
        let path = cap[1].to_string();
        let line: usize = cap[3].parse().unwrap_or(0);
        refs.push(FileRef { path, line });
    }

    refs.sort_by(|a, b| a.path.cmp(&b.path));
    refs.dedup_by(|a, b| a.path == b.path && a.line == b.line);
    refs
}

fn extract_keywords(text: &str) -> Vec<String> {
    let stopwords = [
        "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had",
        "do", "does", "did", "will", "would", "could", "should", "may", "might", "must", "shall",
        "can", "need", "dare", "ought", "used", "to", "of", "in", "for", "on", "with", "at", "by",
        "from", "as", "into", "through", "during", "before", "after", "above", "below", "between",
        "under", "again", "further", "then", "once", "here", "there", "when", "where", "why",
        "how", "all", "each", "few", "more", "most", "other", "some", "such", "no", "nor", "not",
        "only", "own", "same", "so", "than", "too", "very", "just", "and", "but", "if", "or",
        "because", "until", "while", "this", "that", "these", "those", "bug", "fix", "error",
        "issue", "problem", "broken", "please", "add", "make", "update", "change", "need",
    ];

    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.len() > 3)
        .filter(|w| !stopwords.contains(&w.to_lowercase().as_str()))
        .map(|s| s.to_string())
        .collect()
}

fn grep_codebase(dir: &Path, pattern: &str) -> Result<String> {
    let output = Command::new("rg")
        .args([
            "--max-count",
            "5",
            "-n",
            "--no-heading",
            "-g",
            "!*.lock",
            "-g",
            "!node_modules",
            "-g",
            "!target",
            "-g",
            "!vendor",
            "-g",
            "!.git",
            pattern,
        ])
        .current_dir(dir)
        .output()
        .context("Failed to run ripgrep")?;

    let result = String::from_utf8_lossy(&output.stdout);
    // Limit output
    let limited: String = result.lines().take(30).collect::<Vec<_>>().join("\n");
    Ok(limited)
}

async fn read_file_around_line(
    dir: &Path,
    path: &str,
    line: usize,
    context_lines: usize,
) -> Result<String> {
    let file_path = dir.join(path);
    let content = tokio::fs::read_to_string(&file_path).await?;
    let lines: Vec<&str> = content.lines().collect();

    let start = line.saturating_sub(context_lines);
    let end = (line + context_lines).min(lines.len());

    let mut output = String::new();
    for (i, l) in lines[start..end].iter().enumerate() {
        let line_num = start + i + 1;
        let marker = if line_num == line { ">>>" } else { "   " };
        output.push_str(&format!("{} {:4}: {}\n", marker, line_num, l));
    }

    Ok(output)
}

async fn read_file_with_limit(dir: &Path, path: &str, max_lines: usize) -> Result<String> {
    let file_path = dir.join(path);
    let content = tokio::fs::read_to_string(&file_path).await?;
    let lines: Vec<&str> = content.lines().take(max_lines).collect();

    let mut output = lines.join("\n");
    if content.lines().count() > max_lines {
        output.push_str(&format!(
            "\n... ({} more lines)",
            content.lines().count() - max_lines
        ));
    }

    Ok(output)
}

fn get_project_structure(dir: &Path) -> Result<String> {
    // Get top-level files and directories
    let output = Command::new("ls")
        .args(["-la"])
        .current_dir(dir)
        .output()
        .context("Failed to list directory")?;

    let mut structure = String::from_utf8_lossy(&output.stdout).to_string();

    // If it's a Rust project, show src structure
    if dir.join("Cargo.toml").exists() {
        if let Ok(src_output) = Command::new("find")
            .args(["src", "-name", "*.rs", "-type", "f"])
            .current_dir(dir)
            .output()
        {
            structure.push_str("\nRust source files:\n");
            structure.push_str(&String::from_utf8_lossy(&src_output.stdout));
        }
    }

    Ok(structure)
}
