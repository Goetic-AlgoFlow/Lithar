<script setup lang="ts">
import { Button, injectNotificationManager, NavTabs, useVIntl } from '@modrinth/ui'
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import { openPath } from '@/helpers/utils.js'

import {
	callServers,
	type LocalServer,
	type NetworkAddresses,
	serverPath,
	type ServerStatus,
	shareAddress,
} from './api'
import { localServerMessages as messages } from './messages'
import ServerForm from './ServerForm.vue'

const { handleError } = injectNotificationManager()
const { formatMessage } = useVIntl()
const route = useRoute()
const router = useRouter()

const name = computed(() => String(route.params.name ?? ''))
const onSettings = computed(() => route.path.endsWith('/settings'))
const tabs = computed(() => [
	{ label: formatMessage(messages.console), href: serverPath(name.value) },
	{ label: formatMessage(messages.settings), href: `${serverPath(name.value)}/settings` },
])

const server = ref<LocalServer | null>(null)
const missing = ref(false)
const edited = ref<LocalServer | null>(null)
const network = ref<NetworkAddresses>()
const running = ref(false)
const lines = ref<string[]>([])
const busy = ref(false)
const saved = ref(false)

const address = computed(() => server.value && shareAddress(network.value, server.value.port))

async function load() {
	const all = await callServers<LocalServer[]>('list').catch((e) => (handleError(e), []))
	server.value = all.find((s) => s.name === name.value) ?? null
	missing.value = !server.value
	edited.value = server.value ? { ...server.value } : null
	saved.value = false
}

const consoleBox = ref<HTMLElement | null>(null)

async function poll() {
	const status = await callServers<ServerStatus>('status', { name: name.value }).catch(() => null)
	if (!status) return
	running.value = status.running
	const box = consoleBox.value
	const atEnd = !box || box.scrollHeight - box.scrollTop - box.clientHeight < 24
	lines.value = status.lines.slice(-500)
	if (atEnd) nextTick(() => box?.scrollTo({ top: box.scrollHeight }))
}

/** The colour of a console line, from its level as Minecraft servers write it. */
function lineClass(line: string) {
	if (/\/(ERROR|FATAL)\]|\bERROR\b|panicked/.test(line)) return 'text-red'
	if (/\/WARN\]|\bWARN\b/.test(line)) return 'text-orange'
	if (/Done \(|joined the game|left the game/.test(line)) return 'text-green'
	return ''
}

async function act(command: 'start' | 'stop' | 'restart') {
	busy.value = true
	try {
		if (command !== 'start') await callServers('stop', { name: name.value })
		if (command !== 'stop') await callServers('start', { name: name.value })
	} catch (e) {
		handleError(e as Error)
	} finally {
		busy.value = false
		await poll()
	}
}

const line = ref('')
const history = ref<string[]>([])
let historyAt = -1

async function send() {
	const text = line.value.trim()
	if (!text) return
	await callServers('command', { name: name.value, line: text }).catch(handleError)
	if (history.value[0] !== text) history.value.unshift(text)
	history.value.length = Math.min(history.value.length, 50)
	historyAt = -1
	line.value = ''
}

function browseHistory(step: 1 | -1) {
	historyAt = Math.max(-1, Math.min(history.value.length - 1, historyAt + step))
	line.value = historyAt < 0 ? '' : history.value[historyAt]
}

async function save() {
	if (!edited.value) return
	busy.value = true
	try {
		await callServers('save', { server: edited.value, acceptEula: false })
		server.value = { ...edited.value }
		saved.value = true
	} catch (e) {
		handleError(e as Error)
	} finally {
		busy.value = false
	}
}

async function removeServer() {
	if (!window.confirm(formatMessage(messages.deleteConfirm, { name: name.value }))) return
	try {
		await callServers('delete', { name: name.value })
		await router.push('/local-servers')
	} catch (e) {
		handleError(e as Error)
	}
}

async function openFolder() {
	const path = await callServers<string>('folder', { name: name.value }).catch(handleError)
	if (path) await openPath(path).catch(handleError)
}

let timer: ReturnType<typeof setInterval> | undefined
onMounted(async () => {
	network.value = await callServers<NetworkAddresses>('network').catch(() => undefined)
	timer = setInterval(poll, 1500)
})
onBeforeUnmount(() => clearInterval(timer))
watch(
	name,
	async () => {
		lines.value = []
		history.value = []
		await load()
		await poll()
	},
	{ immediate: true },
)
watch(onSettings, (settings) => settings && load())
</script>

<template>
	<div class="p-6 flex flex-col gap-4 max-w-5xl">
		<router-link to="/local-servers" class="text-sm text-secondary hover:underline">
			← {{ formatMessage(messages.allServers) }}
		</router-link>
		<p v-if="missing" class="m-0">{{ formatMessage(messages.notFound, { name }) }}</p>
		<template v-else-if="server">
			<header class="flex flex-wrap items-center gap-3">
				<span
					class="h-3 w-3 rounded-full"
					:class="running ? 'bg-brand' : 'bg-surface-5'"
					aria-hidden="true"
				/>
				<h1 class="m-0 text-2xl font-bold text-contrast">{{ server.name }}</h1>
				<span class="text-secondary">
					{{ server.kind }} {{ server.version }} ·
					{{ formatMessage(running ? messages.running : messages.stopped) }}
				</span>
			</header>
			<p v-if="address" class="m-0">
				{{ formatMessage(messages.shareAddress) }}
				<strong class="text-contrast">{{ address }}</strong>
			</p>
			<p class="m-0 text-sm">
				{{ formatMessage(server.kind === 'oxyra' ? messages.oxyraMods : messages.pluginsHint) }}
			</p>
			<div class="flex flex-wrap gap-2">
				<Button type="colored" color="brand" :disabled="busy || running" @click="act('start')">{{
					formatMessage(messages.start)
				}}</Button>
				<Button :disabled="busy || !running" @click="act('restart')">{{
					formatMessage(messages.restart)
				}}</Button>
				<Button :disabled="busy || !running" @click="act('stop')">{{
					formatMessage(messages.stop)
				}}</Button>
				<Button @click="openFolder">{{ formatMessage(messages.openFolder) }}</Button>
				<Button type="colored" color="red" :disabled="busy || running" @click="removeServer">{{
					formatMessage(messages.delete)
				}}</Button>
			</div>

			<NavTabs :links="tabs" />

			<section v-if="!onSettings" class="flex flex-col gap-2">
				<div
					ref="consoleBox"
					class="h-[28rem] overflow-auto rounded-xl bg-surface-2 p-3 font-mono text-xs"
					role="log"
					aria-live="polite"
				>
					<p v-if="!lines.length" class="m-0 text-secondary">
						{{ formatMessage(messages.consoleEmpty) }}
					</p>
					<div
						v-for="(l, i) in lines"
						:key="i"
						class="whitespace-pre-wrap break-all"
						:class="lineClass(l)"
					>
						{{ l }}
					</div>
				</div>
				<form class="flex gap-2" @submit.prevent="send">
					<input
						v-model="line"
						class="flex-1 font-mono"
						:placeholder="formatMessage(messages.consolePlaceholder)"
						:disabled="!running"
						:aria-label="formatMessage(messages.consolePlaceholder)"
						@keydown.up.prevent="browseHistory(1)"
						@keydown.down.prevent="browseHistory(-1)"
					/>
					<Button native-type="submit" :disabled="!running">{{
						formatMessage(messages.send)
					}}</Button>
				</form>
				<p class="m-0 text-xs text-secondary">{{ formatMessage(messages.consoleHint) }}</p>
			</section>

			<section v-else-if="edited" class="card flex flex-col gap-3">
				<ServerForm v-model="edited" editing />
				<p v-if="running" class="m-0 text-sm">{{ formatMessage(messages.restartToApply) }}</p>
				<div class="flex items-center gap-3">
					<Button type="colored" color="brand" :disabled="busy || !edited.version" @click="save">{{
						formatMessage(busy ? messages.settingUp : messages.save)
					}}</Button>
					<span v-if="saved" class="text-sm text-green">{{ formatMessage(messages.saved) }}</span>
				</div>
			</section>
		</template>
	</div>
</template>
