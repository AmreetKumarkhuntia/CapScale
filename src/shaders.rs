//! WGSL files on disk: loading, and noticing when one is saved.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

const COMMON: &str = "common";

pub struct ShaderDir {
    dir: PathBuf,
    seen: HashMap<String, Option<SystemTime>>,
}

impl ShaderDir {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            seen: HashMap::new(),
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.wgsl"))
    }

    /// `common.wgsl` followed by `<name>.wgsl`. WGSL has no include, so this is how
    /// every pass gets the shared uniform block, bindings and vertex shader.
    pub fn source(&self, name: &str) -> Result<String, String> {
        let read = |n: &str| {
            let path = self.path(n);
            fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
        };
        Ok(format!("{}\n{}", read(COMMON)?, read(name)?))
    }

    /// Indices into `names` of the files saved since the last call. A change to
    /// common.wgsl reports every name. The first call only takes a snapshot.
    pub fn changed(&mut self, names: &[&str]) -> Vec<usize> {
        let mut stamp = |name: &str| {
            let now = fs::metadata(self.dir.join(format!("{name}.wgsl")))
                .and_then(|m| m.modified())
                .ok();
            match self.seen.insert(name.to_string(), now) {
                Some(before) => before != now,
                None => false,
            }
        };
        let common = stamp(COMMON);
        (0..names.len())
            .filter(|&i| stamp(names[i]) || common)
            .collect()
    }
}
