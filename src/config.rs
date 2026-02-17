use std::env;
use std::net::SocketAddr;
use std::time::Duration;

use crate::error::AppError;

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8787";
const DEFAULT_AUTH_REQUIRED: bool = true;
const DEFAULT_READ_TIMEOUT_SECS: u64 = 10;
const DEFAULT_WRITE_TIMEOUT_SECS: u64 = 20;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub auth_required: bool,
    pub api_key: Option<String>,
    pub remindctl_bin: String,
    pub read_timeout: Duration,
    pub write_timeout: Duration,
}

impl Config {
    pub fn from_env() -> Result<Self, AppError> {
        let bind_addr = env::var("BIND_ADDR")
            .unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_owned())
            .parse::<SocketAddr>()
            .map_err(|_| AppError::invalid_config("invalid BIND_ADDR, expected host:port"))?;

        let auth_required = parse_bool_env("AUTH_REQUIRED", DEFAULT_AUTH_REQUIRED)?;
        let api_key = env::var("API_KEY").ok().filter(|value| !value.is_empty());

        if auth_required && api_key.is_none() {
            return Err(AppError::invalid_config(
                "API_KEY must be set when AUTH_REQUIRED=true",
            ));
        }

        let remindctl_bin = env::var("REMINDCTL_BIN").unwrap_or_else(|_| "remindctl".to_owned());

        let read_timeout = Duration::from_secs(parse_u64_env(
            "REMINDCTL_READ_TIMEOUT_SECS",
            DEFAULT_READ_TIMEOUT_SECS,
        )?);
        let write_timeout = Duration::from_secs(parse_u64_env(
            "REMINDCTL_WRITE_TIMEOUT_SECS",
            DEFAULT_WRITE_TIMEOUT_SECS,
        )?);

        Ok(Self {
            bind_addr,
            auth_required,
            api_key,
            remindctl_bin,
            read_timeout,
            write_timeout,
        })
    }

    pub fn log_startup(&self) {
        tracing::info!(
            auth_required = self.auth_required,
            bind_addr = %self.bind_addr,
            remindctl_bin = %self.remindctl_bin,
            read_timeout_secs = self.read_timeout.as_secs(),
            write_timeout_secs = self.write_timeout.as_secs(),
            "starting remindctl mcp server",
        );

        if !self.auth_required {
            tracing::warn!("AUTH_REQUIRED=false, API key auth is disabled");
        }
    }
}

fn parse_bool_env(key: &str, default: bool) -> Result<bool, AppError> {
    match env::var(key) {
        Ok(value) => match value.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(AppError::invalid_config(format!(
                "invalid {key} value, expected true or false"
            ))),
        },
        Err(_) => Ok(default),
    }
}

fn parse_u64_env(key: &str, default: u64) -> Result<u64, AppError> {
    match env::var(key) {
        Ok(value) => value
            .parse::<u64>()
            .map_err(|_| AppError::invalid_config(format!("invalid {key} value"))),
        Err(_) => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bool_rejects_invalid_values() {
        // SAFETY: unit test process-level env mutation for isolated key.
        unsafe {
            env::set_var("AUTH_REQUIRED", "yes");
        }
        let result = parse_bool_env("AUTH_REQUIRED", true);
        assert!(result.is_err(), "invalid boolean env value must fail");
    }
}
