use std::{
    fs,
    path::Path,
};

use anyhow::{
    Context,
    Result,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub database: DatabaseConfig,
    pub services: ServicesConfig,
    pub models: ModelsConfig,
    pub tailscale: TailscaleConfig,
    pub grpc: GrpcConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub path: String,
    pub chat_db_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicesConfig {
    pub poll_interval_ms: u64,
    pub training_set_sample_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsConfig {
    pub embedding_model: String,
    pub emotion_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailscaleConfig {
    pub hostname: String,
    pub state_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcConfig {
    pub port: u16,
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())
            .context(format!("Failed to read config file: {:?}", path.as_ref()))?;
        let config: Config = toml::from_str(&content).context("Failed to parse config file")?;
        Ok(config)
    }

    pub fn default_config() -> Self {
        Self {
            database: DatabaseConfig {
                path: "./us.db".to_string(),
                chat_db_path: "./crates/lib/chat.db".to_string(),
            },
            services: ServicesConfig {
                poll_interval_ms: 1000,
                training_set_sample_rate: 0.05,
            },
            models: ModelsConfig {
                embedding_model: "Qwen/Qwen3-Embedding-0.6B".to_string(),
                emotion_model: "SamLowe/roberta-base-go_emotions".to_string(),
            },
            tailscale: TailscaleConfig {
                hostname: "lonni-daemon".to_string(),
                state_dir: "./tailscale-state".to_string(),
            },
            grpc: GrpcConfig { port: 50051 },
        }
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = toml::to_string_pretty(self).context("Failed to serialize config")?;
        fs::write(path.as_ref(), content)
            .context(format!("Failed to write config file: {:?}", path.as_ref()))?;
        Ok(())
    }
}
