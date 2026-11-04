// Copyright 2026 Joseph Anthony Abbott III
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Throwaway directories of Cargo packages, for tests that run real Cargo
//! on a deliberately broken workspace. They hold only path packages, so
//! Cargo needs no network.

use crate::metadata::{Cargo, output};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

/// A directory under the system's temporary directory, removed on drop.
pub(crate) struct Fixture {
    root: PathBuf,
}

impl Fixture {
    /// A new, empty directory for the test `name`.
    pub(crate) fn new(name: &str) -> Fixture {
        let root = std::env::temp_dir().join(format!("hardshell-xtask-{}-{name}", process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Fixture { root }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Writes `text` to `path`, relative to the fixture, making its
    /// directories.
    pub(crate) fn write(&self, path: &str, text: &str) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// Cargo for the workspace in `dir`, after generating its lockfile.
    pub(crate) fn cargo(&self, dir: &str) -> Cargo {
        let manifest = self.root.join(dir).join("Cargo.toml");
        let cargo = Cargo::new(manifest.clone());
        let mut lock = Command::new(cargo.program());
        lock.args(["generate-lockfile", "--offline", "--manifest-path"])
            .arg(&manifest);
        output(lock).unwrap();
        cargo
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
