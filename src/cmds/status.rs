use crate::cache;
use crate::config;
use crate::status::EnvironmentStatus;
use crate::sums;
use anyhow::{Context, Result};
use clap::Parser;
use std::io::{self, Write};
use std::path::PathBuf;

/// Reports the status of the development environment
///
/// Exits 0 when the environment is up to date, 1 when the environment is stale,
/// and 2 when the environment is unbuilt, or when an error occurs.
#[derive(Debug, Parser)]
pub struct Command {
    /// The directory in which to build
    dir: Option<PathBuf>,
}

impl Command {
    pub fn run(&self) -> Result<u8> {
        let config = config::Config::load(self.dir.as_ref()).context("could not load config")?;
        let stdout = io::stdout();
        let mut handle = stdout.lock();

        let sums_now = sums::Checksums::from(
            &config.build_dir,
            &config.watch_files().context("could not get watch files")?,
        )
        .context("could not calculate checksums")?;
        let cache_file = config.cache_file(&sums_now);
        let cache_file_fallback = config.cache_file_most_recent();

        let status = match cache::Cache::load_with_fallback(&cache_file, &cache_file_fallback) {
            Ok(cache) => {
                if sums::equal(&sums_now, &cache.sums) {
                    EnvironmentStatus::Okay
                } else {
                    EnvironmentStatus::Stale
                }
            }
            Err(_) => EnvironmentStatus::Unknown,
        };

        writeln!(&mut handle, "{}", status).context("could not write status")?;
        Ok(status.code())
    }
}
