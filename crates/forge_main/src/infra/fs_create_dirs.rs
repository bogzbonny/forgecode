use std::path::Path;

use crate::app::FileDirectoryInfra;

#[derive(Default)]
pub struct ForgeCreateDirsService;

#[async_trait::async_trait]
impl FileDirectoryInfra for ForgeCreateDirsService {
    async fn create_dirs(&self, path: &Path) -> anyhow::Result<()> {
        Ok(crate::forge_fs::ForgeFS::create_dir_all(path).await?)
    }
}
