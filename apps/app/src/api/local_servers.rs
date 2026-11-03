use crate::api::Result;
use theseus::local_server::{self, LocalServer, NetworkAddresses, ServerKind, ServerStatus};

pub fn init<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
	tauri::plugin::Builder::new("local-servers")
		.invoke_handler(tauri::generate_handler![
			local_servers_list,
			local_servers_versions,
			local_servers_save,
			local_servers_start,
			local_servers_stop,
			local_servers_command,
			local_servers_status,
			local_servers_delete,
			local_servers_network,
			local_servers_folder,
		])
		.build()
}

#[tauri::command]
pub async fn local_servers_list() -> Result<Vec<LocalServer>> {
	Ok(local_server::list().await?)
}

#[tauri::command]
pub async fn local_servers_versions(kind: ServerKind) -> Result<Vec<String>> {
	Ok(local_server::versions(kind).await?)
}

#[tauri::command]
pub async fn local_servers_save(server: LocalServer, accept_eula: bool) -> Result<()> {
	Ok(local_server::save(server, accept_eula).await?)
}

#[tauri::command]
pub async fn local_servers_start(name: &str) -> Result<()> {
	Ok(local_server::start(name).await?)
}

#[tauri::command]
pub async fn local_servers_stop(name: &str) -> Result<()> {
	Ok(local_server::stop(name).await?)
}

#[tauri::command]
pub async fn local_servers_command(name: &str, line: &str) -> Result<()> {
	Ok(local_server::command(name, line).await?)
}

#[tauri::command]
pub async fn local_servers_status(name: &str) -> Result<ServerStatus> {
	Ok(local_server::status(name).await?)
}

#[tauri::command]
pub async fn local_servers_delete(name: &str) -> Result<()> {
	Ok(local_server::delete(name).await?)
}

#[tauri::command]
pub async fn local_servers_network() -> Result<NetworkAddresses> {
	Ok(local_server::network().await?)
}

#[tauri::command]
pub async fn local_servers_folder(name: &str) -> Result<std::path::PathBuf> {
	Ok(local_server::folder(name).await?)
}
