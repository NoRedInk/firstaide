use crypto_hash::{hex_digest, Algorithm};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
pub struct Checksums(Vec<Checksum>);

impl Checksums {
    pub fn from<R, T>(root: R, filenames: &[T]) -> io::Result<Self>
    where
        R: AsRef<Path>,
        T: AsRef<Path>,
    {
        let mut sums = Vec::new();
        for filename in filenames {
            let sum = Checksum::from(root.as_ref(), filename)?;
            sums.push(sum);
        }
        Ok(Self(sums))
    }

    pub fn sig(&self) -> String {
        // Default bincode config is unlimited so should not error, hence
        // unwrapping is safe.
        hex_digest(Algorithm::SHA1, &bincode::serialize(self).unwrap())
    }
}

impl IntoIterator for Checksums {
    type Item = Checksum;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[derive(Serialize, Deserialize, PartialEq)]
pub enum Checksum {
    Found(PathBuf, Sha1),
    NotFound(PathBuf),
}

impl Checksum {
    pub fn from<T>(root: &Path, filename: T) -> io::Result<Self>
    where
        T: AsRef<Path>,
    {
        // Store the path relative to `root` so that checksums – and the cache
        // signature derived from them – do not depend on where the working
        // tree lives. Paths outside `root` are kept as they are.
        let path = filename.as_ref();
        let path = path.strip_prefix(root).unwrap_or(path).to_path_buf();
        match Sha1::from(&filename) {
            Ok(sha1) => Ok(Checksum::Found(path, sha1)),
            Err(ref err) if err.kind() == io::ErrorKind::NotFound => Ok(Checksum::NotFound(path)),
            Err(err) => Err(err),
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            Checksum::Found(path, _) => path,
            Checksum::NotFound(path) => path,
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq)]
pub struct Sha1(pub String);

impl Sha1 {
    pub fn from<T>(filename: T) -> io::Result<Self>
    where
        T: AsRef<Path>,
    {
        Ok(Self(hex_digest(Algorithm::SHA1, &fs::read(filename)?)))
    }
}

pub fn equal(a: &Checksums, b: &Checksums) -> bool {
    a.0.iter().eq(b.0.iter())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn checksums_of_identical_trees_at_different_roots_are_equal() {
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        for dir in [dir_a.path(), dir_b.path()] {
            fs::create_dir(dir.join("sub")).unwrap();
            fs::write(dir.join("shell.nix"), b"{ }: 12345").unwrap();
            fs::write(dir.join("sub").join("deps.nix"), b"{ }: 67890").unwrap();
        }
        let files_a = [
            dir_a.path().join("shell.nix"),
            dir_a.path().join("sub/deps.nix"),
        ];
        let files_b = [
            dir_b.path().join("shell.nix"),
            dir_b.path().join("sub/deps.nix"),
        ];

        let sums_a = Checksums::from(dir_a.path(), &files_a).unwrap();
        let sums_b = Checksums::from(dir_b.path(), &files_b).unwrap();

        assert!(equal(&sums_a, &sums_b));
        assert_eq!(sums_a.sig(), sums_b.sig());
    }

    #[test]
    fn checksums_of_missing_files_are_also_root_relative() {
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();

        let sums_a = Checksums::from(dir_a.path(), &[dir_a.path().join("nope.nix")]).unwrap();
        let sums_b = Checksums::from(dir_b.path(), &[dir_b.path().join("nope.nix")]).unwrap();

        assert!(equal(&sums_a, &sums_b));
        assert_eq!(sums_a.sig(), sums_b.sig());
    }

    #[test]
    fn checksums_keep_paths_outside_the_root_absolute() {
        let root = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        fs::write(elsewhere.path().join("other.nix"), b"{ }: 1").unwrap();
        let outside = elsewhere.path().join("other.nix");

        let sums = Checksums::from(root.path(), &[outside.clone()]).unwrap();

        assert_eq!(sums.0[0].path(), outside.as_path());
    }
}
