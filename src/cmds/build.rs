use crate::cache;
use crate::config;
use crate::env;
use crate::sums;
use anyhow::{bail, Context, Result};
use clap::Parser;
use spinners::{Spinner, Spinners};
use std::fs;
use std::io::Write;
use std::os::unix;
use std::path::PathBuf;
use tempfile;

/// Builds the development environment and captures its environment variables
#[derive(Debug, Parser)]
pub struct Command {
    /// The directory in which to build
    dir: Option<PathBuf>,
}

impl Command {
    pub fn run(&self) -> Result<u8> {
        let config = config::Config::load(self.dir.as_ref())?;
        build(config)
    }
}

fn spin<F, T>(f: F) -> T
where
    F: FnOnce() -> T,
{
    if atty::is(atty::Stream::Stdout) {
        let mut spinner = Spinner::new(Spinners::Dots, "".into());
        let result = f();
        spinner.stop();
        print!("\x08\x08"); // Backspace over the spinner.
        result
    } else {
        f()
    }
}

fn build(config: config::Config) -> Result<u8> {
    // 0. Check `direnv` is new enough. Older versions have bugs that prevent
    // building from working correctly.
    check_direnv_version(&config).context("could not check direnv version")?;

    // 1. Allow `direnv`.
    log::info!("Allow direnv in {:?}.", &config.build_dir);
    if !config
        .command_to_allow_direnv()
        .status()
        .context("could not run `direnv allow`")?
        .success()
    {
        bail!("could not enable direnv");
    }

    // 2. Create output directory.
    log::info!("Create cache dir at {:?}.", &config.cache_dir);
    fs::create_dir_all(&config.cache_dir).context("could not create cache dir")?;

    // Setting up additional OS pipes for subprocesses to communicate back to us
    // is not well supported in the Rust standard library, so we use files in a
    // temporary directory instead.
    let temp_dir = tempfile::TempDir::new_in(&config.cache_dir)
        .context("could not create temporary directory")?;
    let temp_path = temp_dir.path().to_owned();

    // 3a. Capture outside environment.
    log::info!("Capture outside environment.");
    let env_outside: env::Env = spin(|| {
        let dump_path = temp_path.join("outside");
        env::capture(&dump_path, config.command_to_dump_env_outside(&dump_path))
    })
    .context("could not capture outside environment")?;

    // 3b. Capture inside environment.
    log::info!("Capture inside environment (may involve a full build).");
    let env_inside: env::Env = spin(|| {
        let dump_path = temp_path.join("inside");
        env::capture(
            &dump_path,
            config.command_to_dump_env_inside(&dump_path, &env_outside),
        )
    })
    .context("could not capture inside environment")?;

    // 4. Calculate environment diff.
    log::info!("Calculate environment diff.");
    let env_diff = env::diff(&env_outside, &env_inside);

    // 5. Calculate checksums.
    log::info!("Calculate file checksums.");
    let checksums = spin(|| sums::Checksums::from(&config.build_dir, &config.watch_files()?))
        .context("could not calculate checksums")?;
    let cache_file = config.cache_file(&checksums);

    // 6. Write out cache.
    log::info!("Write out cache.");
    let cache = cache::Cache {
        diff: env_diff,
        sums: checksums,
    };
    cache.save(&cache_file).context("could not save cache")?;

    // 7. Update the most recent cache file link.
    log::info!("Update most recent cache file link.");
    {
        // Write a new symlink into the temporary directory.
        let cache_file_link = temp_path.join("cache");
        unix::fs::symlink(&cache_file, &cache_file_link)
            .context("could not create cache file link")?;
        // Atomically replace any existing symlink with the new one.
        fs::rename(&cache_file_link, &config.cache_file_most_recent())
            .context("could not replace existing symlink with the new one")?
    }

    // 8. Write to the build log. This may be a useful record, but, since we
    // also arrange for direnv to watch this log, it's actually here to prompt
    // direnv to reload. Previously we relied upon getting direnv to watch the
    // cache file, but the cache file is now named with a checksum suffix, so it
    // doesn't notice it.
    {
        let mut build_log = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&config.build_log_file())
            .context("could not open build log file")?;
        let build_time = chrono::offset::Local::now();
        writeln!(
            &mut build_log,
            "{}  {}",
            &build_time.format("%+"),
            &cache_file.display()
        )
        .context("could not write log line")?;
        build_log.sync_all().context("could not sync build log")?;
    }

    // Done.
    Ok(0)
}

fn check_direnv_version(config: &config::Config) -> Result<()> {
    let version_min = semver::Version::new(2, 21, 2);
    let current_version = current_direnv_version(config)?;

    if current_version < version_min {
        bail!(
            concat!(
                "direnv is too old ({}); upgrade to {} or later.\n",
                "--> Nix: nix-channel --update && nix-env --install direnv && nix-env --upgrade direnv\n",
                "--> Homebrew: brew update && brew install direnv && brew upgrade direnv",
            ),
            current_version,
            version_min,
        );
    } else {
        Ok(())
    }
}

fn current_direnv_version(config: &config::Config) -> Result<semver::Version> {
    let mut command = config.command_direnv();
    command.arg("version");
    let command_output = command
        .output()
        .context("could not read `direnv version` output")?;

    let version_string = String::from_utf8_lossy(&command_output.stdout);
    semver::Version::parse(version_string.trim()).context("could not parse version")
}
