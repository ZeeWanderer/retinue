use crate::work::Budget;
use std::fs::{self, File};
use std::io::{self, Seek, SeekFrom, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

pub struct Directory {
    path: PathBuf,
    budget: Option<Budget>,
}

pub struct RecordingFile {
    file: File,
    budget: Option<Budget>,
}

impl Directory {
    pub fn create(path: &Path) -> Result<Self, String> {
        let parent = path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        fs::DirBuilder::new()
            .mode(0o700)
            .create(path)
            .map_err(|error| format!("could not create new capture {}: {error}", path.display()))?;
        Ok(Self {
            path: path.to_owned(),
            budget: None,
        })
    }

    pub fn with_budget(mut self, budget: Budget) -> Self {
        self.budget = Some(budget);
        self
    }

    pub fn file(&self, name: &str) -> Result<RecordingFile, String> {
        if let Some(budget) = &self.budget {
            budget.check().map_err(|error| error.to_string())?;
        }
        Ok(RecordingFile {
            file: self.open(name)?,
            budget: self.budget.clone(),
        })
    }

    fn open(&self, name: &str) -> Result<File, String> {
        let mut parts = Path::new(name).components();
        if !matches!(parts.next(), Some(Component::Normal(_))) || parts.next().is_some() {
            return Err("recording file must be a single filename".into());
        }
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(self.path.join(name))
            .map_err(|error| format!("could not create {name}: {error}"))
    }

    pub fn write(&self, name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
        self.file(name)?
            .write_all(bytes)
            .map_err(|error| format!("could not write {name}: {error}"))?;
        Ok(self.path.join(name))
    }

    pub fn write_report(&self, name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
        // A bounded terminal report must survive cancellation or an exhausted data budget.
        if bytes.len() > 16 * 1024 {
            return Err("capture terminal report exceeds 16 KiB".into());
        }
        self.open(name)?
            .write_all(bytes)
            .map_err(|error| error.to_string())?;
        Ok(self.path.join(name))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Write for RecordingFile {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let bytes = &bytes[..bytes.len().min(64 * 1024)];
        if let Some(budget) = &self.budget {
            budget.write(bytes.len())?;
        }
        let written = self.file.write(bytes)?;
        if let Some(budget) = &self.budget {
            budget.check()?;
        }
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()?;
        if let Some(budget) = &self.budget {
            budget.check()?;
        }
        Ok(())
    }
}

impl Seek for RecordingFile {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.file.seek(position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn recordings_are_private_exclusive_and_confined() {
        let path = std::env::temp_dir().join(format!("wf-recording-{}", std::process::id()));
        let output = Directory::create(&path).unwrap();
        let file = output.write("data.bin", b"original").unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(Directory::create(&path).is_err());
        assert!(output.write("data.bin", b"replacement").is_err());
        assert!(output.file("../escape").is_err());
        assert_eq!(fs::read(file).unwrap(), b"original");
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn exhausted_writer_still_allows_a_small_terminal_report() {
        let path =
            std::env::temp_dir().join(format!("wf-limited-recording-{}", std::process::id()));
        let budget = Budget::new(crate::work::Limits {
            duration: std::time::Duration::from_secs(30),
            read_bytes: 0,
            write_bytes: 4,
        });
        let output = Directory::create(&path)
            .unwrap()
            .with_budget(budget.clone());
        output.write("data.bin", b"four").unwrap();
        assert!(output.write("extra.bin", b"x").is_err());
        budget.cancel();
        assert!(output.file("late.bin").is_err());
        output.write_report("failure.json", b"{}").unwrap();
        assert!(
            output
                .write_report("oversized.json", &vec![0; 16385])
                .is_err()
        );
        assert_eq!(fs::read(path.join("data.bin")).unwrap(), b"four");
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn cancellation_after_writing_is_reported_on_flush() {
        let path = std::env::temp_dir().join(format!("wf-flush-recording-{}", std::process::id()));
        let budget = Budget::new(crate::work::Limits {
            duration: std::time::Duration::from_secs(30),
            read_bytes: 0,
            write_bytes: 4,
        });
        let output = Directory::create(&path)
            .unwrap()
            .with_budget(budget.clone());
        let mut file = output.file("data.bin").unwrap();
        file.write_all(b"four").unwrap();
        budget.cancel();
        assert_eq!(
            file.flush().unwrap_err().kind(),
            io::ErrorKind::ConnectionAborted
        );
        output.write_report("failure.json", b"{}").unwrap();
        fs::remove_dir_all(path).unwrap();
    }
}
