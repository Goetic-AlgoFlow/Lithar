import { invoke } from '@tauri-apps/api/core'

export type ServerKind = 'paper' | 'purpur' | 'oxyra'

export type LocalServer = {
	name: string
	kind: ServerKind
	version: string
	port: number
	memory_mb: number
	motd: string
	max_players: number
	online_mode: boolean
	whitelist: boolean
}

export type ServerStatus = { running: boolean; lines: string[] }

export type NetworkAddresses = { radmin_installed: boolean; radmin: string | null; local: string[] }

/** A command of the `local-servers` plugin (`apps/app/src/api/local_servers.rs`). */
export const callServers = <T>(command: string, args: Record<string, unknown> = {}) =>
	invoke<T>(`plugin:local-servers|local_servers_${command}`, args)

/** The address friends join by: Radmin VPN's first, else the local network's. */
export const shareAddress = (network: NetworkAddresses | undefined, port: number) => {
	const ip = network?.radmin ?? network?.local[0]
	return ip ? `${ip}:${port}` : null
}

export const serverPath = (name: string) => `/local-servers/${encodeURIComponent(name)}`
