use anyhow::{Context, Result};
use path_absolutize::Absolutize;
use serde::Deserialize;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::sums;

#[derive(Debug)]
pub struct Config {
    pub build_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub build_exe: PathBuf,
    pub watch_exe: PathBuf,
    pub direnv_exe: PathBuf,
    pub parent_dir: PathBuf,
    pub self_exe: PathBuf,
    pub messages: Messages,
}

#[derive(Debug, Deserialize)]
struct ConfigData {
    cache_dir: PathBuf,
    build_exe: PathBuf,
    watch_exe: PathBuf,
    #[serde(default)]
    parent_dir: ParentDir,
    #[serde(default)]
    messages: Messages,
}

#[derive(Debug, Deserialize)]
pub struct ParentDir(pub PathBuf);

impl Default for ParentDir {
    fn default() -> Self {
        Self("..".into())
    }
}

impl AsRef<Path> for ParentDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

#[derive(Debug, Deserialize)]
pub struct Messages {
    #[serde(default = "default_getting_started")]
    pub getting_started: String,
    #[serde(default = "default_stale")]
    pub stale: String,
    #[serde(default = "default_inactive")]
    pub inactive: String,
}
impl Default for Messages {
    fn default() -> Self {
        Self {
            getting_started: default_getting_started(),
            stale: default_stale(),
            inactive: default_inactive(),
        }
    }
}

fn default_getting_started() -> String {
    "firstaide --help".to_string()
}
fn default_stale() -> String {
    "firstaide build".to_string()
}
fn default_inactive() -> String {
    "build firstaide and run \"firstaide build\"".to_string()
}

impl Config {
    pub fn load<T: Into<PathBuf>>(dir: Option<T>) -> Result<Config> {
        let dir = match dir {
            Some(d) => {
                let buf = d.into();
                buf.absolutize()
                    .with_context(|| format!("could not absolutize {}", buf.display()))?
                    .to_path_buf()
            }
            None => PathBuf::new()
                .absolutize()
                .context("could not make an absolute path")?
                .to_path_buf(),
        };

        // Find and load a configuration file.
        let config_file: PathBuf = dir
            .ancestors()
            .map(|path| path.join(".firstaide.toml"))
            .find(|path| path.is_file())
            .context("could not find .firstaide.toml")?;
        let config_bytes: Vec<u8> = fs::read(&config_file).context("could not read config file")?;
        let config_data: ConfigData =
            toml::from_slice(&config_bytes).context("could not parse config file as TOML")?;

        // All paths are resolved relative to the directory where we found the
        // configuration file.
        let datum_dir = config_file
            .parent()
            .context("could not get directory of configuration file")?;

        Ok(Config {
            build_dir: datum_dir.to_path_buf(),
            cache_dir: datum_dir
                .join(config_data.cache_dir)
                .absolutize()
                .context("could not make an absolute path to the cache directory")?
                .to_path_buf(),
            build_exe: datum_dir
                .join(config_data.build_exe)
                .absolutize()
                .context("could not make an absolute path to the executables directory")?
                .to_path_buf(),
            watch_exe: datum_dir
                .join(config_data.watch_exe)
                .absolutize()
                .context("could not make an absolute path to the watch directory")?
                .to_path_buf(),
            direnv_exe: search_path("direnv").context("could not find `direnv` on the path")?,
            parent_dir: datum_dir
                .join(config_data.parent_dir)
                .absolutize()
                .context("could not make an absolute path to the parent directory")?
                .to_path_buf(),
            self_exe: env::current_exe().context("could not get the current executable name")?,
            messages: config_data.messages,
        })
    }

    pub fn command_to_allow_direnv(&self) -> Command {
        let mut command = self.command_direnv();
        command.arg("allow").arg(&self.build_dir);
        command
    }

    /// Capture the environment from outside of the Nix environment.
    ///
    /// We also ask direnv to load as if from the *parent* of the build
    /// directory. We do want to include all parent `.envrc`s, but we do NOT
    /// want the `.envrc` in the build directory itself from influencing the
    /// build: *it* is a consumer of *this*, not the other way around or some
    /// weird ouroboros of both.
    pub fn command_to_dump_env_outside<T: Into<PathBuf>>(&self, out: T) -> Command {
        let mut command = self.command_direnv();
        command
            .current_dir(&self.build_dir)
            .arg("exec")
            .arg(&self.parent_dir)
            .arg(&self.self_exe)
            .arg("env")
            .arg("--out")
            .arg(out.into());
        command
    }

    /// Capture the environment from inside the Nix environment.
    ///
    /// We invoke the build with exactly the environment captured from the
    /// outside, which should include parent `.envrc`s.
    pub fn command_to_dump_env_inside<T: Into<PathBuf>>(
        &self,
        out: T,
        env: &[crate::env::Item],
    ) -> Command {
        let mut command = Command::new(&self.build_exe);
        command
            .current_dir(&self.build_dir)
            .arg(&self.self_exe)
            .arg("env")
            .arg("--out")
            .arg(out.into())
            .env_clear()
            .envs(env.iter().cloned());
        command
    }

    pub fn watch_files(&self) -> io::Result<Vec<PathBuf>> {
        let mut command = Command::new(&self.watch_exe);
        command.current_dir(&self.build_dir);
        let output = command.output()?;
        let names = output
            .stdout
            .split(|&byte| byte == 0)
            .filter(|name| !name.is_empty());
        let paths = names.map(OsStr::from_bytes);
        Ok(paths.map(|path| self.abspath(path)).collect())
    }

    /// Return an absolute path, resolved relative to `self.build_dir`.
    fn abspath<T: AsRef<Path>>(&self, path: T) -> PathBuf {
        let p = path.as_ref();
        if p.is_relative() {
            self.build_dir.join(p)
        } else {
            p.to_path_buf()
        }
    }

    pub fn command_direnv(&self) -> Command {
        Command::new(&self.direnv_exe)
    }

    pub fn cache_file(&self, sums: &sums::Checksums) -> PathBuf {
        self.cache_dir.join(format!("cache.{}", sums.sig()))
    }

    pub fn cache_file_most_recent(&self) -> PathBuf {
        self.cache_dir.join("cache")
    }

    pub fn build_log_file(&self) -> PathBuf {
        self.cache_dir.join("build.log")
    }
}

fn search_path<T: Into<PathBuf>>(name: T) -> Option<PathBuf> {
    let name = name.into();
    let home = dirs::home_dir().unwrap_or_else(|| "/home/not/found".into());
    let path = env::var_os("PATH").unwrap_or_default();
    env::split_paths(&path)
        .map(|path| expand_path(path, &home))
        .map(|path| path.join(&name))
        .find(|qpath| qpath.is_file())
}

fn expand_path<T: Into<PathBuf>>(path: T, home: &Path) -> PathBuf {
    use std::path::Component::Normal;

    let path = path.into();
    let tilde = OsStr::new("~");
    let mut components = path.components();
    match components.next() {
        Some(Normal(part)) if part == tilde => home.join(components),
        _ => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_path_with_leading_tilde() {
        assert_eq!(pb("/home/dir/baz"), expand_path("~/baz", &pb("/home/dir")),);
    }

    #[test]
    fn does_not_expand_path_with_leading_tilde_and_username() {
        assert_eq!(pb("~user/baz"), expand_path("~user/baz", &pb("/home/dir")),);
    }

    #[test]
    fn does_not_expand_path_without_leading_tilde() {
        assert_eq!(pb("sum/were"), expand_path("sum/were", &pb("/home/dir")),);
    }

    fn pb<T: Into<PathBuf>>(path: T) -> PathBuf {
        path.into()
    }
}
