use crate::backend;
use crate::config::Config;
use crate::output;
use anyhow::{Context, Result};
use colored::Colorize;
use serde::Deserialize;
use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

/// Regex for extracting file:line references from issue text
static FILE_REF_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"([a-zA-Z0-9_/.-]+\.(rs|rb|py|js|ts|go|java|c|cpp|h|hpp|tsx|jsx)):(\d+)")
        .unwrap()
});

#[derive(Debug, Deserialize)]
struct GitHubIssue {
    number: u64,
    title: String,
    body: Option<String>,
    labels: Vec<GitHubLabel>,
    state: String,
}

#[derive(Debug, Deserialize)]
struct GitHubLabel {
    name: String,
}

pub async fn run(
    config: &Config,
    dir: &Path,
    issue_ref: &str,
    backend_filter: Option<&str>,
    dry_run: bool,
) -> Result<()> {
    // Parse issue reference (number or URL)
    let issue_number = parse_issue_ref(issue_ref)?;

    println!(
        "{} Fetching issue #{}...",
        "fix:".cyan().bold(),
        issue_number
    );

    // Fetch issue details
    let issue = fetch_issue(dir, issue_number)?;

    println!();
    println!("{}", "=".repeat(50).dimmed());
    println!(
        "{} #{}: {}",
        "Issue".cyan().bold(),
        issue.number,
        issue.title
    );
    println!("{}", "=".repeat(50).dimmed());

    if issue.state != "OPEN" {
        println!(
            "{}",
            format!("Warning: Issue is {} (not open)", issue.state).yellow()
        );
    }

    let labels: Vec<&str> = issue.labels.iter().map(|l| l.name.as_str()).collect();
    if !labels.is_empty() {
        println!("{}: {}", "Labels".dimmed(), labels.join(", "));
    }

    println!();
    if let Some(ref body) = issue.body {
        let preview = if body.len() > 500 {
            format!("{}...", crate::utils::truncate_utf8(body, 500))
        } else {
            body.clone()
        };
        println!("{}", preview.dimmed());
    }
    println!();

    // Gather relevant code context based on issue content
    let code_context = gather_code_context(dir, &issue).await?;

    // Build the fix prompt
    let prompt = build_fix_prompt(&issue, &code_context);

    println!(
        "{} Analyzing issue and generating fix...",
        "fix:".cyan().bold()
    );
    println!();

    // Get backends
    let backends = backend::get_backends(config, backend_filter)?;

    // Run query
    let results = backend::run_query(&backends, &prompt, dir, config).await?;
    output::print_results(&results);

    // If not dry run, try to apply the fix
    if !dry_run {
        println!();
        println!(
            "{}",
            "To apply changes, review the suggestions above and edit files manually.".yellow()
        );
        println!(
            "{}",
            "Future: --apply flag will attempt to apply changes automatically.".dimmed()
        );
    }

    Ok(())
}

fn parse_issue_ref(issue_ref: &str) -> Result<u64> {
    // Handle various formats:
    // - "42" or "#42" - just the number
    // - "https://github.com/owner/repo/issues/42" - full URL

    let trimmed = issue_ref.trim().trim_start_matches('#');

    // Try parsing as number first
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

    anyhow::bail!(
        "Invalid issue reference: '{}'. Use issue number (42), #42, or full URL.",
        issue_ref
    )
}

fn fetch_issue(dir: &Path, number: u64) -> Result<GitHubIssue> {
    let output = Command::new("gh")
        .args([
            "issue",
            "view",
            &number.to_string(),
            "--json",
            "number,title,body,labels,state",
        ])
        .current_dir(dir)
        .output()
        .context("Failed to run gh command")?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to fetch issue #{}: {}", number, err.trim());
    }

    let issue: GitHubIssue =
        serde_json::from_slice(&output.stdout).context("Failed to parse issue JSON")?;

    Ok(issue)
}

async fn gather_code_context(dir: &Path, issue: &GitHubIssue) -> Result<String> {
    let mut context = String::new();

    // Extract file references from issue body
    let body = issue.body.as_deref().unwrap_or("");
    let all_text = format!("{} {}", issue.title, body);

    // Look for file:line patterns like "src/main.rs:42" or "main.rs line 42"
    let file_refs = extract_file_references(&all_text);

    // Try to read each referenced file - if it doesn't exist, just skip it
    let mut loaded: Vec<(String, usize, String)> = Vec::new();
    for file_ref in &file_refs {
        if let Ok(content) = tokio::fs::read_to_string(dir.join(&file_ref.path)).await {
            loaded.push((file_ref.path.clone(), file_ref.line, content));
        }
    }
    context.push_str(&render_issue_file_sections(&loaded));

    // Also try to grep for keywords from the issue title
    let keywords = extract_keywords(&issue.title);
    if !keywords.is_empty() && context.is_empty() {
        // Only grep if we didn't find explicit file references
        context.push_str("## Potentially relevant code (from keyword search):\n\n");

        for keyword in keywords.iter().take(3) {
            if let Ok(grep_result) = grep_codebase(dir, keyword) {
                if !grep_result.is_empty() {
                    context.push_str(&format!("### Matches for '{}':\n", keyword));
                    context.push_str("```\n");
                    // Limit grep output
                    let limited: String =
                        grep_result.lines().take(20).collect::<Vec<_>>().join("\n");
                    context.push_str(&limited);
                    context.push_str("\n```\n\n");
                }
            }
        }
    }

    Ok(context)
}

#[derive(Debug)]
struct FileRef {
    path: String,
    line: usize,
}

/// Assemble the "Referenced files from issue" block from already-read contents.
///
/// Returns an empty string when no file yields a window. That matters beyond
/// cosmetics: the caller's keyword-search fallback only runs while `context` is
/// still empty, so emitting a bare heading here would suppress it whenever every
/// reference is stale or out of range.
fn render_issue_file_sections(files: &[(String, usize, String)]) -> String {
    let mut sections = String::new();
    for (path, line, content) in files {
        let lines: Vec<&str> = content.lines().collect();
        let Some(body) = crate::utils::render_line_window(&lines, *line, 10) else {
            continue;
        };

        sections.push_str(&format!("### {}", path));
        if *line > 0 {
            sections.push_str(&format!(" (around line {})", line));
        }
        sections.push_str("\n```\n");
        sections.push_str(&body);
        sections.push_str("```\n\n");
    }

    if sections.is_empty() {
        return sections;
    }
    format!("## Referenced files from issue:\n\n{}", sections)
}

fn extract_file_references(text: &str) -> Vec<FileRef> {
    let mut refs = Vec::new();

    for cap in FILE_REF_RE.captures_iter(text) {
        let path = cap[1].to_string();
        let line: usize = cap[3].parse().unwrap_or(0);
        refs.push(FileRef { path, line });
    }

    // Dedupe
    refs.sort_by(|a, b| a.path.cmp(&b.path));
    refs.dedup_by(|a, b| a.path == b.path && a.line == b.line);

    refs
}

fn extract_keywords(title: &str) -> Vec<String> {
    // Extract meaningful keywords from title for searching
    let stopwords = [
        "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had",
        "do", "does", "did", "will", "would", "could", "should", "may", "might", "must", "shall",
        "can", "need", "dare", "ought", "used", "to", "of", "in", "for", "on", "with", "at", "by",
        "from", "as", "into", "through", "during", "before", "after", "above", "below", "between",
        "under", "again", "further", "then", "once", "here", "there", "when", "where", "why",
        "how", "all", "each", "few", "more", "most", "other", "some", "such", "no", "nor", "not",
        "only", "own", "same", "so", "than", "too", "very", "just", "and", "but", "if", "or",
        "because", "until", "while", "this", "that", "these", "those", "bug", "fix", "error",
        "issue", "problem", "broken",
    ];

    title
        .split(|c: char| !c.is_alphanumeric() && c != '_')
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
            pattern,
        ])
        .current_dir(dir)
        .output()
        .context("Failed to run ripgrep")?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn build_fix_prompt(issue: &GitHubIssue, code_context: &str) -> String {
    let body = issue.body.as_deref().unwrap_or("(no description)");

    format!(
        r#"You are fixing a GitHub issue. Analyze the issue and provide a fix.

## Issue #{}: {}

{}

{}

## Instructions

1. Analyze the issue description and any referenced code
2. Identify the root cause
3. Provide a specific fix with code changes
4. Show the exact changes needed (before/after or unified diff format)
5. Explain why this fix addresses the issue

If you need more context about specific files, say which files you'd need to see.

Provide your fix:"#,
        issue.number, issue.title, body, code_context
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, line: usize, body: &str) -> (String, usize, String) {
        (path.to_string(), line, body.to_string())
    }

    #[test]
    fn issue_sections_all_out_of_range_yields_nothing() {
        // The point of returning "" rather than a bare heading: the caller only
        // runs its keyword-search fallback while `context` is still empty.
        let files = vec![
            file("src/main.rs", 99999, "a\nb\nc\n"),
            file("src/lib.rs", usize::MAX, "x\n"),
        ];
        assert_eq!(render_issue_file_sections(&files), "");
    }

    #[test]
    fn issue_sections_empty_input_yields_nothing() {
        assert_eq!(render_issue_file_sections(&[]), "");
    }

    #[test]
    fn issue_sections_mixed_emits_heading_once_and_only_valid_sections() {
        let files = vec![
            file("src/gone.rs", 99999, "a\nb\n"),
            file("src/real.rs", 2, "a\nb\nc\n"),
            file("src/also_gone.rs", 40000, "z\n"),
        ];
        let out = render_issue_file_sections(&files);

        assert_eq!(out.matches("## Referenced files from issue:").count(), 1);
        assert_eq!(out.matches("### ").count(), 1);
        assert!(out.contains("### src/real.rs (around line 2)"));
        assert!(!out.contains("gone.rs"));
        assert!(!out.contains("```\n```"), "no empty fenced block");
    }

    #[test]
    fn issue_sections_line_zero_omits_the_around_line_suffix() {
        let out = render_issue_file_sections(&[file("src/a.rs", 0, "x\ny\n")]);
        assert!(out.contains("### src/a.rs\n```\n"));
        assert!(!out.contains("around line"));
    }

    #[test]
    fn issue_sections_framing_is_unchanged() {
        let out = render_issue_file_sections(&[file("src/a.rs", 1, "x\n")]);
        assert_eq!(
            out,
            "## Referenced files from issue:\n\n### src/a.rs (around line 1)\n```\n>>>    1: x\n```\n\n"
        );
    }

    #[test]
    fn issue_sections_empty_file_contributes_nothing() {
        assert_eq!(
            render_issue_file_sections(&[file("src/empty.rs", 1, "")]),
            ""
        );
    }

    fn issue(title: &str, body: &str) -> GitHubIssue {
        GitHubIssue {
            number: 1,
            title: title.to_string(),
            body: Some(body.to_string()),
            labels: vec![],
            state: "OPEN".to_string(),
        }
    }

    /// Drives the real `gather_code_context` rather than inferring from the
    /// builder's return value: the fallback gate is `context.is_empty()`, so a
    /// bare heading pushed before it silently disables keyword search.
    #[tokio::test]
    async fn stale_reference_still_reaches_the_keyword_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/main.rs"), "one\ntwo\nthree\n").unwrap();

        // The file exists and reads fine; only the line number is out of range.
        let issue = issue(
            "Slicing panics during truncation helpers",
            "Broken at src/main.rs:99999 after the refactor",
        );

        let context = gather_code_context(tmp.path(), &issue).await.unwrap();

        assert!(
            !context.contains("## Referenced files from issue:"),
            "a heading with no sections under it must not be emitted"
        );
        assert!(
            context.contains("## Potentially relevant code (from keyword search):"),
            "keyword fallback must fire when every reference collapses to empty, got: {context:?}"
        );
    }

    #[tokio::test]
    async fn valid_reference_suppresses_the_keyword_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/main.rs"), "one\ntwo\nthree\n").unwrap();

        let issue = issue(
            "Slicing panics during truncation helpers",
            "Broken at src/main.rs:2 after the refactor",
        );

        let context = gather_code_context(tmp.path(), &issue).await.unwrap();

        assert!(context.contains("## Referenced files from issue:"));
        assert!(context.contains(">>>    2: two"));
        assert!(
            !context.contains("## Potentially relevant code"),
            "fallback must stay suppressed when a real section rendered"
        );
    }
}
