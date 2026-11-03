<script setup lang="ts">
import { Button, Checkbox, injectNotificationManager, useVIntl } from '@modrinth/ui'
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'

import {
	callServers,
	type LocalServer,
	type NetworkAddresses,
	serverPath,
	type ServerStatus,
} from './api'
import { localServerMessages as messages } from './messages'
import ServerForm from './ServerForm.vue'

const { handleError } = injectNotificationManager()
const { formatMessage } = useVIntl()
const router = useRouter()

const servers = ref<LocalServer[]>([])
const running = ref<Record<string, boolean>>({})
const network = ref<NetworkAddresses>()
const busy = ref(false)
const eula = ref(false)

const blank = (): LocalServer => ({
	name: '',
	kind: 'paper',
	version: '',
	port: 25565,
	memory_mb: 4096,
	motd: formatMessage(messages.defaultMotd),
	max_players: 10,
	online_mode: true,
	whitelist: false,
})
const form = ref<LocalServer>(blank())

async function refresh() {
	servers.value = await callServers<LocalServer[]>('list').catch((e) => (handleError(e), []))
	const states = await Promise.all(
		servers.value.map((s) =>
			callServers<ServerStatus>('status', { name: s.name }).catch(() => null),
		),
	)
	running.value = Object.fromEntries(servers.value.map((s, i) => [s.name, !!states[i]?.running]))
}

async function create() {
	busy.value = true
	try {
		await callServers('save', { server: form.value, acceptEula: eula.value })
		const name = form.value.name
		form.value = blank()
		eula.value = false
		await router.push(serverPath(name))
	} catch (e) {
		handleError(e as Error)
	} finally {
		busy.value = false
	}
}

let timer: ReturnType<typeof setInterval> | undefined
onMounted(async () => {
	await refresh()
	network.value = await callServers<NetworkAddresses>('network').catch(() => undefined)
	timer = setInterval(refresh, 3000)
})
onBeforeUnmount(() => clearInterval(timer))
</script>

<template>
	<div class="p-6 flex flex-col gap-4 max-w-4xl">
		<h1 class="m-0 text-2xl font-bold text-contrast">{{ formatMessage(messages.title) }}</h1>

		<section class="card flex flex-col gap-2">
			<h2 class="m-0 text-lg text-contrast">{{ formatMessage(messages.network) }}</h2>
			<p v-if="network?.radmin" class="m-0">
				{{ formatMessage(messages.radminAddress) }}
				<strong class="text-contrast">{{ network.radmin }}</strong
				>.
				{{ formatMessage(messages.radminJoin) }}
			</p>
			<p v-else-if="network?.radmin_installed" class="m-0">
				{{ formatMessage(messages.radminOffline) }}
			</p>
			<p v-else class="m-0">
				{{ formatMessage(messages.radminMissing) }}
			</p>
			<p class="m-0 text-sm">{{ formatMessage(messages.firewall) }}</p>
			<p v-if="network?.local.length" class="m-0">
				{{ formatMessage(messages.localNetwork, { addresses: network.local.join(', ') }) }}
			</p>
		</section>

		<section class="card flex flex-col gap-3">
			<h2 class="m-0 text-lg text-contrast">{{ formatMessage(messages.yourServers) }}</h2>
			<p v-if="!servers.length" class="m-0">{{ formatMessage(messages.none) }}</p>
			<div class="grid gap-3 sm:grid-cols-2">
				<router-link
					v-for="s in servers"
					:key="s.name"
					:to="serverPath(s.name)"
					class="flex flex-col gap-1 rounded-xl bg-surface-2 p-4 text-primary hover:brightness-110"
				>
					<span class="flex items-center gap-2 font-bold text-contrast">
						<span
							class="h-2 w-2 rounded-full"
							:class="running[s.name] ? 'bg-brand' : 'bg-surface-5'"
							aria-hidden="true"
						/>
						{{ s.name }}
					</span>
					<span class="text-sm">
						{{ s.kind }} {{ s.version }} · :{{ s.port }} ·
						{{ formatMessage(running[s.name] ? messages.running : messages.stopped) }}
					</span>
				</router-link>
			</div>
		</section>

		<section class="card flex flex-col gap-3">
			<h2 class="m-0 text-lg text-contrast">{{ formatMessage(messages.newServer) }}</h2>
			<ServerForm v-model="form" />
			<Checkbox v-model="eula" :label="formatMessage(messages.eula)" />
			<Button
				type="colored"
				color="brand"
				:disabled="busy || !eula || !form.name || !form.version"
				@click="create"
			>
				{{ formatMessage(busy ? messages.settingUp : messages.create) }}
			</Button>
		</section>
	</div>
</template>
