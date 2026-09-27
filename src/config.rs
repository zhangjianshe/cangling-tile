use clap::{Parser, Subcommand};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
};

pub fn management_db_path(config_dir: &Path) -> PathBuf {
    let current = config_dir.join("cangling-tile-meta.db");
    let legacy = config_dir.join("tile-cache-meta.db");
    if !current.exists() && legacy.exists() {
        if let Err(error) = std::fs::rename(&legacy, &current) {
            eprintln!(
                "unable to migrate {} to {}: {error}",
                legacy.display(),
                current.display()
            );
            return legacy;
        }
    }
    current
}

#[derive(Clone, Debug, Parser)]
#[command(version, about)]
pub struct Config {
    #[command(subcommand)]
    pub command: Option<Command>,
    /// HTTP listen address.
    #[arg(long, env = "CANGLING_TILE_ADDR", default_value = "0.0.0.0:7601")]
    pub addr: SocketAddr,

    /// Tokio asynchronous runtime worker threads.
    #[arg(long, env = "CANGLING_TILE_WORKER_THREADS", default_value_t = 4)]
    pub worker_threads: usize,

    /// Root directory containing all tile database directories.
    #[arg(long, env = "CANGLING_TILE_ROOT", default_value = "./tiledata")]
    pub root_dir: PathBuf,

    /// Instance-local directory containing the management database.
    #[arg(
        long,
        env = "CANGLING_TILE_CONFIG_DIR",
        default_value = "./config",
        global = true
    )]
    pub config_dir: PathBuf,

    /// Bearer token required by write and management APIs.
    #[arg(long, env = "CANGLING_TILE_AUTH_TOKEN", default_value = "")]
    pub auth_token: String,

    /// Explicitly allow unauthenticated writes. Intended only for isolated development.
    #[arg(
        long,
        env = "CANGLING_TILE_ALLOW_UNAUTHENTICATED_WRITES",
        default_value_t = false
    )]
    pub allow_unauthenticated_writes: bool,

    /// Mark Dashboard session cookies Secure when served behind HTTPS.
    #[arg(long, env = "CANGLING_TILE_SECURE_COOKIES", default_value_t = false)]
    pub secure_cookies: bool,

    /// Initial dashboard administrator password; only used when no administrator exists.
    #[arg(long, env = "CANGLING_TILE_ADMIN_PASSWORD")]
    pub admin_password: Option<String>,

    /// Maximum accepted tile body in bytes.
    #[arg(
        long,
        env = "CANGLING_TILE_MAX_TILE_BYTES",
        default_value_t = 33_554_432
    )]
    pub max_tile_bytes: usize,

    /// Maximum in-memory tile LRU size in bytes. Zero disables it.
    #[arg(
        long,
        env = "CANGLING_TILE_MEMORY_CACHE_BYTES",
        default_value_t = 536_870_912
    )]
    pub memory_cache_bytes: usize,

    /// Do not admit a single tile larger than this many bytes into the memory LRU.
    #[arg(
        long,
        env = "CANGLING_TILE_MEMORY_CACHE_MAX_TILE_BYTES",
        default_value_t = 2_097_152
    )]
    pub memory_cache_max_tile_bytes: usize,

    /// Evict memory-cached tiles not accessed for this many seconds. Zero disables TTL eviction.
    #[arg(
        long,
        env = "CANGLING_TILE_MEMORY_CACHE_IDLE_SECONDS",
        default_value_t = 10_800
    )]
    pub memory_cache_idle_seconds: u64,

    /// Bounded write queue capacity; overload is rejected with HTTP 503.
    #[arg(long, env = "CANGLING_TILE_WRITE_QUEUE", default_value_t = 4096)]
    pub write_queue: usize,

    /// Maximum SQLite read connections per open shard.
    #[arg(long, env = "CANGLING_TILE_READ_CONNECTIONS", default_value_t = 4)]
    pub read_connections: u32,

    /// Close an unused SQLite shard after this many seconds. Zero disables idle closing.
    #[arg(long, env = "CANGLING_TILE_SHARD_IDLE_SECONDS", default_value_t = 300)]
    pub shard_idle_seconds: u64,

    /// Daily cleanup hour in Asia/Shanghai local time (0-23).
    #[arg(long, env = "CANGLING_TILE_CLEANUP_HOUR", default_value_t = 4)]
    pub cleanup_hour: u8,

    /// Delete entries not accessed for this many days. Zero disables cleanup.
    #[arg(long, env = "CANGLING_TILE_RETENTION_DAYS", default_value_t = 7)]
    pub retention_days: u64,
}

#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    /// Reset or create the dashboard administrator password.
    ResetPassword {
        /// New password; omitted to generate and print one once.
        #[arg(short, long)]
        password: Option<String>,
    },
    /// Check the local HTTP health endpoint and exit.
    Healthcheck,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn management_database_migrates_legacy_name() {
        let directory = tempfile::tempdir().unwrap();
        let legacy = directory.path().join("tile-cache-meta.db");
        std::fs::write(&legacy, b"legacy").unwrap();

        let current = management_db_path(directory.path());

        assert_eq!(current, directory.path().join("cangling-tile-meta.db"));
        assert_eq!(std::fs::read(current).unwrap(), b"legacy");
        assert!(!legacy.exists());
    }
}
