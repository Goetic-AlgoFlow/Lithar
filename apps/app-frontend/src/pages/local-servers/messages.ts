import { defineMessages } from '@modrinth/ui'

export const localServerMessages = defineMessages({
	title: {
		id: 'app.local-servers.title',
		defaultMessage: 'Local servers',
	},
	network: {
		id: 'app.local-servers.network',
		defaultMessage: 'Network',
	},
	radminAddress: {
		id: 'app.local-servers.radmin-address',
		defaultMessage: 'Radmin VPN address:',
	},
	radminJoin: {
		id: 'app.local-servers.radmin-join',
		defaultMessage: 'Friends in your Radmin network join with it.',
	},
	radminOffline: {
		id: 'app.local-servers.radmin-offline',
		defaultMessage:
			'Radmin VPN is installed but not connected: open it and join or create a network.',
	},
	radminMissing: {
		id: 'app.local-servers.radmin-missing',
		defaultMessage:
			"Radmin VPN isn't installed. Get it from radmin-vpn.com, create a network and share its name and password with your friends.",
	},
	localNetwork: {
		id: 'app.local-servers.local-network',
		defaultMessage: 'Local network: {addresses}',
	},
	yourServers: {
		id: 'app.local-servers.your-servers',
		defaultMessage: 'Your servers',
	},
	shareAddress: {
		id: 'app.local-servers.share-address',
		defaultMessage: 'Share this address:',
	},
	oxyraMods: {
		id: 'app.local-servers.oxyra-mods',
		defaultMessage:
			'Runs the newest Oxyra build: the native mods compiled into it run on the server too.',
	},
	pluginsHint: {
		id: 'app.local-servers.plugins-hint',
		defaultMessage: 'Add plugins to the plugins folder, then restart the server.',
	},
	start: {
		id: 'app.local-servers.start',
		defaultMessage: 'Start',
	},
	stop: {
		id: 'app.local-servers.stop',
		defaultMessage: 'Stop',
	},
	openFolder: {
		id: 'app.local-servers.open-folder',
		defaultMessage: 'Open folder',
	},
	delete: {
		id: 'app.local-servers.delete',
		defaultMessage: 'Delete',
	},
	deleteConfirm: {
		id: 'app.local-servers.delete-confirm',
		defaultMessage: 'Delete {name} and its worlds? This cannot be undone.',
	},
	consolePlaceholder: {
		id: 'app.local-servers.console-placeholder',
		defaultMessage: 'Console command',
	},
	send: {
		id: 'app.local-servers.send',
		defaultMessage: 'Send',
	},
	newServer: {
		id: 'app.local-servers.new-server',
		defaultMessage: 'New server',
	},
	name: {
		id: 'app.local-servers.name',
		defaultMessage: 'Name',
	},
	software: {
		id: 'app.local-servers.software',
		defaultMessage: 'Software',
	},
	version: {
		id: 'app.local-servers.version',
		defaultMessage: 'Version',
	},
	port: {
		id: 'app.local-servers.port',
		defaultMessage: 'Port',
	},
	memory: {
		id: 'app.local-servers.memory',
		defaultMessage: 'Memory (MB)',
	},
	maxPlayers: {
		id: 'app.local-servers.max-players',
		defaultMessage: 'Maximum players',
	},
	motd: {
		id: 'app.local-servers.motd',
		defaultMessage: 'Message of the day',
	},
	onlineMode: {
		id: 'app.local-servers.online-mode',
		defaultMessage: 'Check accounts with Mojang (online mode)',
	},
	whitelist: {
		id: 'app.local-servers.whitelist',
		defaultMessage: 'Whitelist only',
	},
	eula: {
		id: 'app.local-servers.eula',
		defaultMessage: 'I accept the Minecraft EULA (aka.ms/MinecraftEULA)',
	},
	settingUp: {
		id: 'app.local-servers.setting-up',
		defaultMessage: 'Setting up…',
	},
	create: {
		id: 'app.local-servers.create',
		defaultMessage: 'Create server',
	},
	firewall: {
		id: 'app.local-servers.firewall',
		defaultMessage:
			"The first time a server starts, Windows asks to let it through the firewall: allow it on public networks too, since Windows often counts Radmin VPN's network as public.",
	},
	defaultMotd: {
		id: 'app.local-servers.default-motd',
		defaultMessage: 'A Lithar server',
	},
	allServers: {
		id: 'app.local-servers.all-servers',
		defaultMessage: 'All servers',
	},
	none: {
		id: 'app.local-servers.none',
		defaultMessage: 'No server yet: make one below.',
	},
	running: {
		id: 'app.local-servers.running',
		defaultMessage: 'Running',
	},
	stopped: {
		id: 'app.local-servers.stopped',
		defaultMessage: 'Stopped',
	},
	restart: {
		id: 'app.local-servers.restart',
		defaultMessage: 'Restart',
	},
	console: {
		id: 'app.local-servers.console',
		defaultMessage: 'Console',
	},
	settings: {
		id: 'app.local-servers.settings',
		defaultMessage: 'Settings',
	},
	save: {
		id: 'app.local-servers.save',
		defaultMessage: 'Save',
	},
	saved: {
		id: 'app.local-servers.saved',
		defaultMessage: 'Saved.',
	},
	restartToApply: {
		id: 'app.local-servers.restart-to-apply',
		defaultMessage: 'The server is running: restart it to apply the new settings.',
	},
	consoleEmpty: {
		id: 'app.local-servers.console-empty',
		defaultMessage: 'Start the server to see its console.',
	},
	consoleHint: {
		id: 'app.local-servers.console-hint',
		defaultMessage: 'Up and down arrows: previous commands.',
	},
	notFound: {
		id: 'app.local-servers.not-found',
		defaultMessage: 'There is no server named {name}.',
	},
})
