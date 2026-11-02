<script setup lang="ts">
import { DownloadIcon, ExternalIcon, FolderOpenIcon, SpinnerIcon, TrashIcon } from '@modrinth/assets'
import {
	Admonition,
	Avatar,
	Button,
	ConfirmModal,
	defineMessages,
	injectNotificationManager,
	Toggle,
	useVIntl,
} from '@modrinth/ui'
import { useMutation, useQuery, useQueryClient } from '@tanstack/vue-query'
import { open } from '@tauri-apps/plugin-dialog'
import { openUrl } from '@tauri-apps/plugin-opener'
import { computed, ref, useTemplateRef } from 'vue'

import {
	get_horizon_mods,
	type HorizonMod,
	install_horizon_mod,
	install_horizon_mod_file,
	remove_horizon_mod,
	set_horizon_mod_enabled,
} from '@/helpers/instance'

const props = defineProps<{ instanceId: string }>()

const { formatMessage } = useVIntl()
const { handleError } = injectNotificationManager()
const queryClient = useQueryClient()
const trustModal = useTemplateRef('trustModal')
const pendingNative = ref<HorizonMod | null>(null)
const pendingFile = ref<string | null>(null)
const busy = ref<string | null>(null)

const queryKey = computed(() => ['instance', props.instanceId, 'horizon-mods'])
const modsQuery = useQuery({
	queryKey,
	queryFn: () => get_horizon_mods(props.instanceId),
})

const installed = computed(() => (modsQuery.data.value ?? []).filter((m) => m.installed_version))
const available = computed(() => (modsQuery.data.value ?? []).filter((m) => !m.installed_version))

const action = useMutation({
	mutationFn: async (run: () => Promise<void>) => await run(),
	onSettled: () => {
		busy.value = null
		void queryClient.invalidateQueries({ queryKey: queryKey.value })
	},
	onError: handleError,
})

function run(mod: HorizonMod, task: () => Promise<void>) {
	busy.value = mod.id
	action.mutate(task)
}

function install(mod: HorizonMod) {
	if (mod.kind === 'native-mod') {
		pendingNative.value = mod
		trustModal.value?.show()
		return
	}
	run(mod, () => install_horizon_mod(props.instanceId, mod.id, false))
}

function installTrusted() {
	const mod = pendingNative.value
	const file = pendingFile.value
	pendingNative.value = null
	pendingFile.value = null
	if (mod) run(mod, () => install_horizon_mod(props.instanceId, mod.id, true))
	if (file) installFile(file, true)
}

function installFile(path: string, trustNativeCode: boolean) {
	busy.value = path
	action.mutate(async () => {
		await install_horizon_mod_file(props.instanceId, path, trustNativeCode)
	})
}

async function pickFile() {
	const path = await open({
		multiple: false,
		filters: [{ name: formatMessage(messages.fileFilter), extensions: ['hmod', 'oxmod', 'hnmod'] }],
	})
	if (typeof path !== 'string') return
	if (/\.(oxmod|hnmod)$/i.test(path)) {
		pendingFile.value = path
		trustModal.value?.show()
		return
	}
	installFile(path, false)
}

const trustDescription = computed(() =>
	pendingFile.value
		? formatMessage(messages.trustFileDescription, {
				name: pendingFile.value.split(/[\\/]/).pop() ?? '',
			})
		: formatMessage(messages.trustDescription, { name: pendingNative.value?.name ?? '' }),
)

const messages = defineMessages({
	title: { id: 'app.horizon-mods.title', defaultMessage: 'Oxyra mods' },
	description: {
		id: 'app.horizon-mods.description',
		defaultMessage:
			"Mods made for Oxyra, from Oxyra's registry. Java mods don't run in Oxyra. Restart the game after a change.",
	},
	installed: { id: 'app.horizon-mods.installed', defaultMessage: 'Installed' },
	available: { id: 'app.horizon-mods.available', defaultMessage: 'Available' },
	none: { id: 'app.horizon-mods.none', defaultMessage: 'No Oxyra mod installed yet.' },
	loadError: {
		id: 'app.horizon-mods.load-error',
		defaultMessage: "Oxyra's mod registry could not be reached.",
	},
	install: { id: 'app.horizon-mods.install', defaultMessage: 'Install' },
	remove: { id: 'app.horizon-mods.remove', defaultMessage: 'Remove' },
	source: { id: 'app.horizon-mods.source', defaultMessage: 'Source' },
	update: { id: 'app.horizon-mods.update', defaultMessage: 'Update to {version}' },
	native: { id: 'app.horizon-mods.native', defaultMessage: 'Native' },
	content: { id: 'app.horizon-mods.content', defaultMessage: 'Content' },
	unsigned: { id: 'app.horizon-mods.unsigned', defaultMessage: 'Unsigned' },
	by: { id: 'app.horizon-mods.by', defaultMessage: 'by {authors}' },
	nativeNotice: {
		id: 'app.horizon-mods.native-notice',
		defaultMessage:
			"Native mods are compiled into this instance's game before its next launch. The first build downloads Rust and takes 20 to 30 minutes; later builds are faster.",
	},
	trustTitle: {
		id: 'app.horizon-mods.trust.title',
		defaultMessage: 'Trust this native mod?',
	},
	trustDescription: {
		id: 'app.horizon-mods.trust.description',
		defaultMessage:
			"**{name}** is native code: it is not sandboxed and runs with full access to this computer. Oxyra's registry reviewed it before listing it, but read its source if you are unsure. It will be compiled into this instance's game before its next launch.",
	},
	installFile: { id: 'app.horizon-mods.install-file', defaultMessage: 'Install from file' },
	requires: { id: 'app.horizon-mods.requires', defaultMessage: 'Requires: {mods}' },
	fileFilter: { id: 'app.horizon-mods.file-filter', defaultMessage: 'Oxyra mods' },
	trustFileDescription: {
		id: 'app.horizon-mods.trust.file-description',
		defaultMessage:
			"**{name}** is native code: it is not sandboxed and runs with full access to this computer. It doesn't come from Oxyra's registry, so nobody reviewed it: install it only if you trust where it comes from. It will be compiled into this instance's game before its next launch.",
	},
	trustProceed: {
		id: 'app.horizon-mods.trust.proceed',
		defaultMessage: 'I trust this mod: install',
	},
})
</script>

<template>
	<div class="flex flex-col gap-4">
		<ConfirmModal
			ref="trustModal"
			:title="formatMessage(messages.trustTitle)"
			:description="trustDescription"
			:proceed-label="formatMessage(messages.trustProceed)"
			@proceed="installTrusted"
		/>
		<div class="flex items-start gap-3">
			<div class="flex flex-1 flex-col gap-1">
				<h2 class="m-0 text-xl font-bold text-contrast">{{ formatMessage(messages.title) }}</h2>
				<p class="m-0 text-secondary">{{ formatMessage(messages.description) }}</p>
			</div>
			<Button :disabled="busy !== null" @click="pickFile">
				<FolderOpenIcon />
				{{ formatMessage(messages.installFile) }}
			</Button>
		</div>
		<Admonition v-if="modsQuery.isError.value" type="critical">
			{{ formatMessage(messages.loadError) }}
		</Admonition>
		<div v-else-if="modsQuery.isPending.value" class="flex justify-center p-8">
			<SpinnerIcon class="h-8 w-8 animate-spin" />
		</div>
		<template v-else>
			<Admonition v-if="installed.some((m) => m.kind === 'native-mod')" type="info">
				{{ formatMessage(messages.nativeNotice) }}
			</Admonition>
			<section class="flex flex-col gap-2">
				<h3 class="m-0 text-lg font-semibold text-contrast">
					{{ formatMessage(messages.installed) }}
				</h3>
				<p v-if="installed.length === 0" class="m-0 text-secondary">
					{{ formatMessage(messages.none) }}
				</p>
				<div
					v-for="mod in installed"
					:key="mod.id"
					class="flex items-center gap-3 rounded-2xl bg-bg-raised p-3"
				>
					<Avatar :src="mod.icon ?? undefined" size="48px" :alt="mod.name" />
					<div class="flex min-w-0 flex-1 flex-col">
						<span class="font-semibold text-contrast">
							{{ mod.name }}
							<span class="text-sm font-normal text-secondary">
								{{ mod.installed_version }}
								·
								{{ formatMessage(mod.kind === 'native-mod' ? messages.native : messages.content) }}
							</span>
						</span>
						<span class="truncate text-sm text-secondary">{{ mod.summary }}</span>
						<span v-if="mod.requires.length > 0" class="text-sm text-secondary">
							{{ formatMessage(messages.requires, { mods: mod.requires.join(', ') }) }}
						</span>
					</div>
					<Button
						v-if="mod.installed_version !== mod.latest_version"
						:disabled="busy !== null"
						@click="install(mod)"
					>
						<DownloadIcon />
						{{ formatMessage(messages.update, { version: mod.latest_version }) }}
					</Button>
					<Toggle
						:model-value="mod.enabled"
						:disabled="busy !== null"
						:aria-label="mod.name"
						@update:model-value="
							run(mod, () => set_horizon_mod_enabled(instanceId, mod.id, $event ?? false))
						"
					/>
					<Button
						icon-only
						:disabled="busy !== null"
						:aria-label="formatMessage(messages.remove)"
						@click="run(mod, () => remove_horizon_mod(instanceId, mod.id))"
					>
						<SpinnerIcon v-if="busy === mod.id" class="animate-spin" />
						<TrashIcon v-else />
					</Button>
				</div>
			</section>
			<section class="flex flex-col gap-2">
				<h3 class="m-0 text-lg font-semibold text-contrast">
					{{ formatMessage(messages.available) }}
				</h3>
				<div
					v-for="mod in available"
					:key="mod.id"
					class="flex items-center gap-3 rounded-2xl bg-bg-raised p-3"
				>
					<Avatar :src="mod.icon ?? undefined" size="48px" :alt="mod.name" />
					<div class="flex min-w-0 flex-1 flex-col">
						<span class="font-semibold text-contrast">
							{{ mod.name }}
							<span class="text-sm font-normal text-secondary">
								{{ mod.latest_version }}
								·
								{{ formatMessage(mod.kind === 'native-mod' ? messages.native : messages.content) }}
								<template v-if="!mod.signed"> · {{ formatMessage(messages.unsigned) }}</template>
							</span>
						</span>
						<span class="truncate text-sm text-secondary">{{ mod.summary }}</span>
						<span v-if="mod.authors.length" class="text-xs text-secondary">
							{{ formatMessage(messages.by, { authors: mod.authors.join(', ') }) }}
						</span>
					</div>
					<Button
						v-if="mod.source"
						icon-only
						:aria-label="formatMessage(messages.source)"
						@click="openUrl(mod.source)"
					>
						<ExternalIcon />
					</Button>
					<Button color="brand" :disabled="busy !== null" @click="install(mod)">
						<SpinnerIcon v-if="busy === mod.id" class="animate-spin" />
						<DownloadIcon v-else />
						{{ formatMessage(messages.install) }}
					</Button>
				</div>
			</section>
		</template>
	</div>
</template>
