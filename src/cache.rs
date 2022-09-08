use std::collections::hash_map::DefaultHasher;
use std::fs::Metadata;
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::autofix;
use cacache::Error::EntryNotFound;
use filetime::FileTime;
use log::error;
use serde::{Deserialize, Serialize};

use crate::message::Message;
use crate::settings::Settings;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, Deserialize)]
struct CacheMetadata {
    mtime: i64,
}

#[derive(Serialize)]
struct CheckResultRef<'a> {
    metadata: &'a CacheMetadata,
    messages: &'a [Message],
}

#[derive(Deserialize)]
struct CheckResult {
    metadata: CacheMetadata,
    messages: Vec<Message>,
}

pub enum Mode {
    ReadWrite,
    ReadOnly,
    WriteOnly,
    None,
}

impl Mode {
    fn allow_read(&self) -> bool {
        match self {
            Mode::ReadWrite => true,
            Mode::ReadOnly => true,
            Mode::WriteOnly => false,
            Mode::None => false,
        }
    }

    fn allow_write(&self) -> bool {
        match self {
            Mode::ReadWrite => true,
            Mode::ReadOnly => false,
            Mode::WriteOnly => true,
            Mode::None => false,
        }
    }
}

impl From<bool> for Mode {
    fn from(value: bool) -> Self {
        match value {
            true => Mode::ReadWrite,
            false => Mode::None,
        }
    }
}

fn cache_dir() -> &'static str {
    "./.ruff_cache"
}

fn cache_key(path: &Path, settings: &Settings, autofix: &autofix::Mode) -> String {
    let mut hasher = DefaultHasher::new();
    settings.hash(&mut hasher);
    autofix.hash(&mut hasher);
    format!(
        "{}@{}@{}",
        path.canonicalize().unwrap().to_string_lossy(),
        VERSION,
        hasher.finish()
    )
}

pub fn get(
    path: &Path,
    metadata: &Metadata,
    settings: &Settings,
    autofix: &autofix::Mode,
    mode: &Mode,
) -> Option<Vec<Message>> {
    if !mode.allow_read() {
        return None;
    };

    match cacache::read_sync(cache_dir(), cache_key(path, settings, autofix)) {
        Ok(encoded) => match bincode::deserialize::<CheckResult>(&encoded[..]) {
            Ok(CheckResult {
                metadata: CacheMetadata { mtime },
                messages,
            }) => {
                if FileTime::from_last_modification_time(metadata).unix_seconds() == mtime {
                    return Some(messages);
                }
            }
            Err(e) => error!("Failed to deserialize encoded cache entry: {e:?}"),
        },
        Err(EntryNotFound(_, _)) => {}
        Err(e) => error!("Failed to read from cache: {e:?}"),
    }
    None
}

pub fn set(
    path: &Path,
    metadata: &Metadata,
    settings: &Settings,
    autofix: &autofix::Mode,
    messages: &[Message],
    mode: &Mode,
) {
    if !mode.allow_write() {
        return;
    };

    let check_result = CheckResultRef {
        metadata: &CacheMetadata {
            mtime: FileTime::from_last_modification_time(metadata).unix_seconds(),
        },
        messages,
    };
    if let Err(e) = cacache::write_sync(
        cache_dir(),
        cache_key(path, settings, autofix),
        bincode::serialize(&check_result).unwrap(),
    ) {
        error!("Failed to write to cache: {e:?}")
    }
}
