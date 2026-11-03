use crate::data::ModLoader;
use crate::launcher::horizon;
use crate::state::CachedEntry;
use crate::{ErrorKind, State};
pub use daedalus::minecraft::VersionManifest;
pub use daedalus::modded::Manifest;

#[tracing::instrument]
pub async fn get_minecraft_versions() -> crate::Result<VersionManifest> {
    let state = State::get().await?;
    let minecraft_versions = CachedEntry::get_minecraft_manifest(
        None,
        &state.pool,
        &state.api_semaphore,
    )
    .await?
    .ok_or(ErrorKind::NoValueFor("minecraft versions".to_string()))?;

    Ok(minecraft_versions)
}

// #[tracing::instrument]
pub async fn get_loader_versions(loader: &str) -> crate::Result<Manifest> {
    if loader == ModLoader::Horizon.as_meta_str() {
        return Ok(horizon_manifest());
    }
    let state = State::get().await?;
    let cache_key =
        daedalus::modded::loader_manifest_metadata(loader).cache_key;
    let loaders = CachedEntry::get_loader_manifest(
        &cache_key,
        None,
        &state.pool,
        &state.api_semaphore,
    )
    .await?
    .ok_or(ErrorKind::NoValueFor(format!("{loader} loader versions")))?;

    Ok(loaders.manifest)
}

/// Horizon is not on the metadata server: it plays one Minecraft version.
fn horizon_manifest() -> Manifest {
    Manifest {
        game_versions: vec![daedalus::modded::Version {
            id: horizon::GAME_VERSION.to_string(),
            stable: true,
            version_group: None,
            // `latest` plays Oxyra; `official` plays Horizon from its own releases.
            loaders: ["latest", horizon::OFFICIAL]
                .into_iter()
                .map(|id| daedalus::modded::LoaderVersion {
                    id: id.to_string(),
                    url: String::new(),
                    stable: true,
                })
                .collect(),
        }],
        version_groups: Vec::new(),
    }
}
