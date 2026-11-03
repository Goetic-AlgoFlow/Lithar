<script setup lang="ts">
import {
	Checkbox,
	Combobox,
	type ComboboxOption,
	injectNotificationManager,
	useVIntl,
} from '@modrinth/ui'
import { computed, ref, watch } from 'vue'

import { callServers, type LocalServer, type ServerKind } from './api'
import { localServerMessages as messages } from './messages'

const props = defineProps<{ editing?: boolean }>()
const server = defineModel<LocalServer>({ required: true })

const { handleError } = injectNotificationManager()
const { formatMessage } = useVIntl()

const versions = ref<string[]>([])

const kinds: ComboboxOption<ServerKind>[] = [
	{ value: 'paper', label: 'Paper' },
	{ value: 'purpur', label: 'Purpur' },
	{ value: 'oxyra', label: 'Oxyra' },
]
const versionOptions = computed<ComboboxOption<string>[]>(() =>
	(props.editing && server.value.version && !versions.value.includes(server.value.version)
		? [server.value.version, ...versions.value]
		: versions.value
	).map((v) => ({ value: v, label: v })),
)

watch(
	() => server.value.kind,
	async (kind, before) => {
		versions.value = []
		versions.value = await callServers<string[]>('versions', { kind }).catch(
			(e) => (handleError(e), []),
		)
		// An edited server keeps its version until its software changes.
		if (!props.editing || before !== undefined) server.value.version = versions.value[0] ?? ''
	},
	{ immediate: true },
)
</script>

<template>
	<div class="flex flex-col gap-3">
		<label v-if="!editing" class="flex flex-col gap-1"
			>{{ formatMessage(messages.name) }}<input v-model="server.name" maxlength="48"
		/></label>
		<div class="flex flex-col gap-1">
			<span>{{ formatMessage(messages.software) }}</span>
			<Combobox v-model="server.kind" :options="kinds" />
		</div>
		<div class="flex flex-col gap-1">
			<span>{{ formatMessage(messages.version) }}</span>
			<Combobox v-model="server.version" :options="versionOptions" searchable />
		</div>
		<label class="flex flex-col gap-1">
			{{ formatMessage(messages.port)
			}}<input v-model.number="server.port" type="number" min="1024" max="65535" />
		</label>
		<label class="flex flex-col gap-1">
			{{ formatMessage(messages.memory)
			}}<input v-model.number="server.memory_mb" type="number" min="512" step="512" />
		</label>
		<label class="flex flex-col gap-1">
			{{ formatMessage(messages.maxPlayers)
			}}<input v-model.number="server.max_players" type="number" min="1" />
		</label>
		<label class="flex flex-col gap-1"
			>{{ formatMessage(messages.motd) }}<input v-model="server.motd"
		/></label>
		<Checkbox v-model="server.online_mode" :label="formatMessage(messages.onlineMode)" />
		<Checkbox v-model="server.whitelist" :label="formatMessage(messages.whitelist)" />
	</div>
</template>
