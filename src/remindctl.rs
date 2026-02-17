use std::process::Stdio;
use std::time::Duration;

use serde::de::DeserializeOwned;
use tokio::process::Command;
use tokio::time;

use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct RemindctlRunner {
    binary: String,
    read_timeout: Duration,
    write_timeout: Duration,
}

impl RemindctlRunner {
    pub fn new(binary: String, read_timeout: Duration, write_timeout: Duration) -> Self {
        Self {
            binary,
            read_timeout,
            write_timeout,
        }
    }

    pub async fn run_read_json<T>(&self, mut args: Vec<String>) -> Result<T, AppError>
    where
        T: DeserializeOwned,
    {
        append_safe_flags(&mut args);
        let output = self.run(args, self.read_timeout).await?;
        serde_json::from_slice::<T>(&output).map_err(AppError::from)
    }

    pub async fn run_write_json<T>(&self, mut args: Vec<String>) -> Result<T, AppError>
    where
        T: DeserializeOwned,
    {
        append_safe_flags(&mut args);
        let output = self.run(args, self.write_timeout).await?;
        serde_json::from_slice::<T>(&output).map_err(AppError::from)
    }

    pub async fn run_write_no_output(&self, mut args: Vec<String>) -> Result<(), AppError> {
        append_safe_flags(&mut args);
        let _ = self.run(args, self.write_timeout).await?;
        Ok(())
    }

    async fn run(&self, args: Vec<String>, timeout: Duration) -> Result<Vec<u8>, AppError> {
        let mut cmd = Command::new(&self.binary);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default());

        let child = cmd.spawn()?;
        let output = time::timeout(timeout, child.wait_with_output())
            .await
            .map_err(|_| AppError::CommandTimeout)??;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(AppError::CommandFailed(stderr));
        }

        Ok(output.stdout)
    }
}

fn append_safe_flags(args: &mut Vec<String>) {
    args.push("--json".to_owned());
    args.push("--no-input".to_owned());
    args.push("--no-color".to_owned());
}
