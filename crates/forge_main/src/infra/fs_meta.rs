use std::path::Path;

use anyhow::Result;
use crate::app::FileInfoInfra;

pub struct ForgeFileMetaService;
#[async_trait::async_trait]
impl FileInfoInfra for ForgeFileMetaService {
    async fn is_file(&self, path: &Path) -> Result<bool> {
        Ok(crate::forge_fs::ForgeFS::is_file(path))
    }

    async fn is_binary(&self, path: &Path) -> Result<bool> {
        crate::forge_fs::ForgeFS::is_binary_file(path).await
    }

    async fn exists(&self, path: &Path) -> Result<bool> {
        Ok(crate::forge_fs::ForgeFS::exists(path))
    }

    async fn file_size(&self, path: &Path) -> Result<u64> {
        crate::forge_fs::ForgeFS::file_size(path).await
    }
}
