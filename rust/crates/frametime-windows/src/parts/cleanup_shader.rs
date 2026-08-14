//! Fixed cache-family selection for standalone cleanup.

use frametime_core::Config;

use crate::{known_folders, resolve_cache_template, shader_cache_delete_qualified};

pub(crate) enum CleanupShaderCacheKind {
    Cs2,
    NvidiaDx,
    NvidiaGl,
    DirectX,
    AmdDx,
}

pub(crate) fn clear_cleanup_shader_cache(
    config: &Config,
    kind: CleanupShaderCacheKind,
) -> Result<usize, String> {
    config
        .validate()
        .map_err(|error| format!("invalid cleanup cache config: {error}"))?;
    if !shader_cache_delete_qualified() {
        return Err(
            "handle-backed shader-cache deletion is not armed without Windows VM qualification"
                .into(),
        );
    }
    let folders = known_folders()?;
    let values: Vec<&str> = match kind {
        CleanupShaderCacheKind::Cs2 => config
            .paths
            .shader_cache
            .iter()
            .map(String::as_str)
            .collect(),
        CleanupShaderCacheKind::NvidiaDx => vec![&config.paths.nvidia_dx_cache],
        CleanupShaderCacheKind::NvidiaGl => vec![&config.paths.nvidia_gl_cache],
        CleanupShaderCacheKind::DirectX => vec![&config.paths.directx_shader_cache],
        CleanupShaderCacheKind::AmdDx => vec![r"%LOCALAPPDATA%\AMD\DxCache"],
    };
    let roots = values
        .into_iter()
        .map(|value| resolve_cache_template(value, &folders))
        .collect::<Result<Vec<_>, _>>()?;
    crate::shader_cache_handles::delete_fixed_roots(&roots)
}
