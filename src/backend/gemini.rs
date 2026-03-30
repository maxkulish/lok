use crate::config::BackendConfig;
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

pub struct GeminiBackend {
    command: String,
    args: Vec<String>,
    skip_lines: usize,
}

impl GeminiBackend {
    pub fn new(config: &BackendConfig) -> Result<Self> {
        let command = config.command.clone().unwrap_or_else(|| "npx".to_string());

        let args = if config.args.is_empty() {
            vec!["@google/gemini-cli".to_string()]
        } else {
            config.args.clone()
        };

        Ok(Self {
            command,
            args,
            skip_lines: config.skip_lines,
        })
    }

    fn parse_output(&self, output: &str) -> String {
        output
            .lines()
            .skip(self.skip_lines)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[async_trait]
impl super::Backend for GeminiBackend {
    fn name(&self) -> &str {
        "gemini"
    }

    async fn query(
        &self,
        prompt: &str,
        cwd: &Path,
        model: Option<&str>,
    ) -> Result<super::QueryOutput> {
        // Gemini CLI requires stdin to be a pipe (not null/tty), so we use shell
        // to pipe empty input: echo '' | npx @google/gemini-cli 'prompt'
        let escaped_prompt = prompt.replace("'", "'\\''");
        let model_flag = model
            .filter(|m| !m.is_empty())
            .map(|m| format!(" --model '{}'", m.replace("'", "'\\''")))
            .unwrap_or_default();
        let shell_cmd = format!(
            "echo '' | {} {}{} '{}'",
            &self.command,
            self.args.join(" "),
            model_flag,
            escaped_prompt
        );

        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(&shell_cmd)
            .current_dir(cwd)
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd
            .output()
            .await
            .context("Failed to execute gemini command")?;

        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            anyhow::bail!("Gemini failed: {}", stderr_str);
        }

        let parsed_stdout = self.parse_output(&stdout);
        Ok(super::QueryOutput::from_process(
            parsed_stdout,
            stderr_str,
            exit_code,
        ))
    }

    fn is_available(&self) -> bool {
        which::which(&self.command).is_ok()
    }
}
