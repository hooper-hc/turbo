use anyhow::{anyhow, bail, Result};
use turbo_tasks::ValueToString;
use turbo_tasks_fs::{rope::RopeBuilder, File, FileContent, FileContentVc, FileSystemPathVc};
use turbopack::{
    module_options::ModuleOptionsContextVc,
    resolve_options_context::ResolveOptionsContextVc,
    transition::{Transition, TransitionVc},
    ModuleAssetContextVc,
};
use turbopack_core::{
    asset::{Asset, AssetVc},
    chunk::{ChunkableAssetVc, ChunkingContextVc},
    compile_time_info::CompileTimeInfoVc,
    virtual_asset::VirtualAssetVc,
};
use turbopack_ecmascript::{chunk_group_files_asset::ChunkGroupFilesAsset, utils::stringify_js};

#[turbo_tasks::value(shared)]
pub struct NextEdgeTransition {
    pub edge_compile_time_info: CompileTimeInfoVc,
    pub edge_chunking_context: ChunkingContextVc,
    pub edge_resolve_options_context: ResolveOptionsContextVc,
    pub output_path: FileSystemPathVc,
    pub base_path: FileSystemPathVc,
    pub bootstrap_file: FileContentVc,
}

#[turbo_tasks::value_impl]
impl Transition for NextEdgeTransition {
    #[turbo_tasks::function]
    async fn process_source(&self, asset: AssetVc) -> Result<AssetVc> {
        let FileContent::Content(base) = &*self.bootstrap_file.await? else {
            bail!("runtime code not found");
        };
        let path = asset.path().await?;
        let path = self
            .base_path
            .await?
            .get_path_to(&path)
            .ok_or_else(|| anyhow!("asset is not in base_path"))?;
        let path = if let Some((name, ext)) = path.rsplit_once('.') {
            if !ext.contains('/') {
                name
            } else {
                path
            }
        } else {
            path
        };
        let mut new_content =
            RopeBuilder::from(format!("const PAGE = {};\n", stringify_js(path)).into_bytes());
        new_content.concat(base.content());
        let file = File::from(new_content.build());
        Ok(VirtualAssetVc::new(
            asset.path().join("next-edge-bootstrap.ts"),
            FileContent::Content(file).cell().into(),
        )
        .into())
    }

    #[turbo_tasks::function]
    fn process_compile_time_info(
        &self,
        _compile_time_info: CompileTimeInfoVc,
    ) -> CompileTimeInfoVc {
        self.edge_compile_time_info
    }

    #[turbo_tasks::function]
    fn process_module_options_context(
        &self,
        context: ModuleOptionsContextVc,
    ) -> ModuleOptionsContextVc {
        context
    }

    #[turbo_tasks::function]
    fn process_resolve_options_context(
        &self,
        _context: ResolveOptionsContextVc,
    ) -> ResolveOptionsContextVc {
        self.edge_resolve_options_context
    }

    #[turbo_tasks::function]
    async fn process_module(
        &self,
        asset: AssetVc,
        _context: ModuleAssetContextVc,
    ) -> Result<AssetVc> {
        let chunkable_asset = match ChunkableAssetVc::resolve_from(asset).await? {
            Some(chunkable_asset) => chunkable_asset,
            None => bail!("asset {} is not chunkable", asset.path().to_string().await?),
        };

        let asset = ChunkGroupFilesAsset {
            asset: chunkable_asset,
            chunking_context: self.edge_chunking_context,
            base_path: self.output_path,
            runtime_entries: None,
        };

        Ok(asset.cell().into())
    }
}
