use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use forge_domain::{Environment, Snapshot, SnapshotRepository};

/// Implementation of the SnapshotService (inlined from forge_snaps)
#[derive(Debug)]
pub struct SnapshotService {
    /// Base directory for storing snapshots
    snapshots_directory: PathBuf,
}

impl SnapshotService {
    /// Create a new FileSystemSnapshotService with a specific home path
    pub fn new(snapshot_base_dir: PathBuf) -> Self {
        Self { snapshots_directory: snapshot_base_dir }
    }
}

impl SnapshotService {
    pub async fn create_snapshot(&self, path: PathBuf) -> Result<Snapshot> {
        let snapshot = Snapshot::create(path)?;

        // Create intermediary directories if they don't exist
        let snapshot_path = snapshot.snapshot_path(Some(self.snapshots_directory.clone()));
        if let Some(parent) = PathBuf::from(&snapshot_path).parent() {
            forge_fs::ForgeFS::create_dir_all(parent).await?;
        }

        let content = forge_fs::ForgeFS::read(&snapshot.path).await?;
        let path = snapshot.snapshot_path(Some(self.snapshots_directory.clone()));
        forge_fs::ForgeFS::write(path, content).await?;
        Ok(snapshot)
    }

    /// Find the most recent snapshot for a given path based on filename
    /// timestamp
    async fn find_recent_snapshot(snapshot_dir: &PathBuf) -> Result<Option<PathBuf>> {
        let mut latest_path = None;
        let mut latest_filename = None;
        let mut dir = forge_fs::ForgeFS::read_dir(&snapshot_dir).await?;

        while let Some(entry) = dir.next_entry().await? {
            let filename = entry.file_name().to_string_lossy().to_string();
            if filename.ends_with(".snap")
                && (latest_filename.is_none() || filename > latest_filename.clone().unwrap())
            {
                latest_filename = Some(filename);
                latest_path = Some(entry.path());
            }
        }

        Ok(latest_path)
    }

    pub async fn undo_snapshot(&self, path: PathBuf) -> Result<()> {
        let snapshot = Snapshot::create(path.clone())?;

        // All the snaps for `path` are stored in `snapshot.path_hash()` directory.
        let snapshot_dir = self.snapshots_directory.join(snapshot.path_hash());

        // Check if the `snapshot_dir` exists
        if !forge_fs::ForgeFS::exists(&snapshot_dir) {
            return Err(anyhow::anyhow!("No snapshots found for {path:?}"));
        }

        // Retrieve the latest snapshot path
        let snapshot_path = Self::find_recent_snapshot(&snapshot_dir)
            .await?
            .context(format!("No valid snapshots found for {path:?}"))?;

        // Restore the content
        let content = forge_fs::ForgeFS::read(&snapshot_path).await?;
        forge_fs::ForgeFS::write(&path, content).await?;

        // Remove the used snapshot
        forge_fs::ForgeFS::remove_file(&snapshot_path).await?;

        Ok(())
    }
}

pub struct ForgeFileSnapshotService {
    inner: Arc<SnapshotService>,
}

impl ForgeFileSnapshotService {
    pub fn new(env: Environment) -> Self {
        Self {
            inner: Arc::new(SnapshotService::new(env.snapshot_path())),
        }
    }
}

#[async_trait::async_trait]
impl SnapshotRepository for ForgeFileSnapshotService {
    // Creation
    async fn insert_snapshot(&self, file_path: &Path) -> Result<Snapshot> {
        self.inner.create_snapshot(file_path.to_path_buf()).await
    }

    // Undo
    async fn undo_snapshot(&self, file_path: &Path) -> Result<()> {
        self.inner.undo_snapshot(file_path.to_path_buf()).await
    }
}
