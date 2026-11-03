<script setup lang="ts">
import { DownloadIcon, ExternalIcon, SpinnerIcon } from '@modrinth/assets'
import {
	Admonition,
	Avatar,
	Button,
	Checkbox,
	ConfirmModal,
	ContentCardLayout as ContentPageLayout,
	type ContentItem,
	defineMessages,
	injectNotificationManager,
	provideContentManager,
	useVIntl,
} from '@modrinth/ui'
import { useMutation, useQuery, useQueryClient } from '@tanstack/vue-query'
import { open } from '@tauri-apps/plugin-dialog'
import { openUrl } from '@tauri-apps/plugin-opener'
import { computed, ref, useTemplateRef } from 'vue'

import {
	get_content_items,
	get_horizon_mods,
	type HorizonMod,
	install_horizon_mod,
	install_horizon_mod_file,
	remove_horizon_mod,
	remove_project,
	set_horizon_mod_enabled,
} from '@/helpers/instance'
import { useAppSettings } from '@/composables/use-app-settings.ts'
import { injectInstancePage } from '@/pages/instance/instance-context'

import { trustMessages } from './horizon-trust-messages'
import { nativeTrustSkipped, skipNativeTrust } from './native-trust'

const props = defineProps<{ instanceId: string }>()
const instancePage = injectInstancePage()
const appSettings = useAppSettings()

const { formatMessage } = useVIntl()
const { handleError } = injectNotificationManager()
const queryClient = useQueryClient()
const trustModal = useTemplateRef('trustModal')
const pendingNative = ref<HorizonMod | null>(null)
const pendingFiles = ref<string[]>([])
const dontAsk = ref(false)
const busy = ref<string | null>(null)

const queryKey = computed(() => ['instance', props.instanceId, 'horizon-mods'])
const modsQuery = useQuery({
	queryKey,
	queryFn: () => get_horizon_mods(props.instanceId),
})

const installed = computed(() => (modsQuery.data.value ?? []).filter((m) => m.installed_version))
const available = computed(() => (modsQuery.data.value ?? []).filter((m) => !m.installed_version))

// Shaders and resource packs installed from Discover: Oxyra reads them from the instance's folders.
const packsKey = computed(() => ['instance', props.instanceId, 'horizon-packs'])
const packsQuery = useQuery({
	queryKey: packsKey,
	queryFn: () => get_content_items(props.instanceId),
})
const packs = computed(() =>
	(packsQuery.data.value ?? []).filter((item) =>
		['shader', 'shaderpack', 'resourcepack'].includes(item.project_type),
	),
)
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
		if (nativeTrustSkipped()) {
			run(mod, () => install_horizon_mod(props.instanceId, mod.id, true))
			return
		}
		pendingNative.value = mod
		dontAsk.value = false
		trustModal.value?.show()
		return
	}
	run(mod, () => install_horizon_mod(props.instanceId, mod.id, false))
}

function installTrusted() {
	if (dontAsk.value) skipNativeTrust().catch(handleError)
	const mod = pendingNative.value
	const files = pendingFiles.value
	pendingNative.value = null
	pendingFiles.value = []
	if (mod) run(mod, () => install_horizon_mod(props.instanceId, mod.id, true))
	if (files.length > 0) installFiles(files, true)
}

/** Installs `paths` one after the other; one that fails doesn't stop the others. */
function installFiles(paths: string[], trustNativeCode: boolean) {
	busy.value = paths[0]
	action.mutate(async () => {
		for (const path of paths) {
			await install_horizon_mod_file(props.instanceId, path, trustNativeCode).catch(handleError)
		}
	})
}

async function pickFile() {
	const picked = await open({
		multiple: true,
		filters: [{ name: formatMessage(messages.fileFilter), extensions: ['hmod', 'oxmod', 'hnmod'] }],
	})
	const paths = (Array.isArray(picked) ? picked : [picked]).filter(
		(path): path is string => typeof path === 'string',
	)
	const native = paths.filter((path) => /\.(oxmod|hnmod)$/i.test(path))
	const plain = paths.filter((path) => !native.includes(path))
	if (plain.length > 0) installFiles(plain, false)
	if (native.length === 0) return
	if (nativeTrustSkipped()) {
		installFiles(native, true)
		return
	}
	pendingFiles.value = native
	dontAsk.value = false
	trustModal.value?.show()
}

/** An installed Oxyra mod's row id, apart from the shaders' and resource packs' file ids. */
const MOD_ROW = 'oxyra-mod:'
const modOf = (item: ContentItem) => installed.value.find((m) => MOD_ROW + m.id === item.id)

function modItem(mod: HorizonMod): ContentItem {
	const outdated = mod.installed_version !== mod.latest_version
	const item: ContentItem = {
		id: MOD_ROW + mod.id,
		file_name: mod.id,
		project_type: 'mod',
		enabled: mod.enabled,
		has_update: outdated,
		update_version_id: outdated ? mod.latest_version : null,
		project: { id: mod.id, slug: mod.id, title: mod.name, icon_url: mod.icon ?? undefined },
		version: {
			id: mod.installed_version ?? '',
			version_number: mod.installed_version ?? '',
			file_name: mod.id,
		},
	}
	return item
}

const items = computed<ContentItem[]>(() => [...installed.value.map(modItem), ...packs.value])

/** Runs `task` for the row `id`, then reloads the mods and the packs. */
async function act(id: string, task: () => Promise<unknown>) {
	busy.value = id
	try {
		await task()
	} catch (error) {
		handleError(error as Error)
	} finally {
		busy.value = null
		await Promise.all([
			queryClient.invalidateQueries({ queryKey: queryKey.value }),
			queryClient.invalidateQueries({ queryKey: packsKey.value }),
		])
	}
}

provideContentManager({
	items,
	loading: computed(() => modsQuery.isPending.value || packsQuery.isPending.value),
	error: computed(() => (modsQuery.error.value as Error | null) ?? null),
	managedContent: ref(null),
	isPackLocked: ref(false),
	isBusy: computed(() => busy.value !== null),
	contentTypeLabel: computed(() => formatMessage(messages.contentType)),
	toggleEnabled: async (item) => {
		const mod = modOf(item)
		if (mod)
			await act(mod.id, () => set_horizon_mod_enabled(props.instanceId, mod.id, !mod.enabled))
	},
	deleteItem: async (item) => {
		const mod = modOf(item)
		if (mod) await act(mod.id, () => remove_horizon_mod(props.instanceId, mod.id))
		else if (item.file_path) {
			const path = item.file_path
			await act(item.id, () => remove_project(props.instanceId, path))
		}
	},
	canToggleItem: (item) => !!modOf(item),
	refresh: async () => {
		await Promise.all([modsQuery.refetch(), packsQuery.refetch()])
	},
	browse: () => instancePage.browseContent('mod'),
	uploadFiles: () => void pickFile(),
	hasUpdateSupport: true,
	updateItem: (id) => {
		const mod = installed.value.find((m) => MOD_ROW + m.id === id)
		if (mod) install(mod)
	},
	mapToTableItem: (item) => {
		const mod = modOf(item)
		return {
			id: item.id,
			project: item.project ?? {
				id: item.file_name,
				slug: null,
				title: item.file_name,
				icon_url: null,
			},
			version: item.version ?? {
				id: item.file_name,
				version_number: '',
				file_name: item.file_name,
			},
			external: !mod && !item.project,
			enabled: mod ? item.enabled : undefined,
			hideToggle: !mod,
			hasUpdate: item.has_update,
		}
	},
})

const trustDescription = computed(() =>
	pendingFiles.value.length > 0
		? formatMessage(messages.trustFileDescription, {
				name: pendingFiles.value.map((path) => path.split(/[\\/]/).pop() ?? path).join(', '),
			})
		: formatMessage(trustMessages.trustDescription, { name: pendingNative.value?.name ?? '' }),
)

const messages = defineMessages({
	contentType: { id: 'app.horizon-mods.content-type', defaultMessage: 'content' },
	available: { id: 'app.horizon-mods.available', defaultMessage: 'Available' },
	loadError: {
		id: 'app.horizon-mods.load-error',
		defaultMessage: "Oxyra's mod registry could not be reached.",
	},
	install: { id: 'app.horizon-mods.install', defaultMessage: 'Install' },
	source: { id: 'app.horizon-mods.source', defaultMessage: 'Source' },
	native: { id: 'app.horizon-mods.native', defaultMessage: 'Native' },
	content: { id: 'app.horizon-mods.content', defaultMessage: 'Content' },
	unsigned: { id: 'app.horizon-mods.unsigned', defaultMessage: 'Unsigned' },
	by: { id: 'app.horizon-mods.by', defaultMessage: 'by {authors}' },
	nativeNotice: {
		id: 'app.horizon-mods.native-notice',
		defaultMessage:
			"Native mods are compiled into this instance's game before its next launch. The first build downloads Rust and takes 20 to 30 minutes; later builds are faster.",
	},
	fileFilter: { id: 'app.horizon-mods.file-filter', defaultMessage: 'Oxyra mods' },
	trustFileDescription: {
		id: 'app.horizon-mods.trust.file-description',
		defaultMessage:
			"**{name}** is native code: it is not sandboxed and runs with full access to this computer. It doesn't come from Oxyra's registry, so nobody reviewed it: install it only if you trust where it comes from. It will be compiled into this instance's game before its next launch.",
	},
})
</script>

<template>
	<div class="flex flex-col gap-4">
		<ConfirmModal
			ref="trustModal"
			:title="formatMessage(trustMessages.trustTitle)"
			:description="trustDescription"
			:proceed-label="formatMessage(trustMessages.trustProceed)"
			@proceed="installTrusted"
		>
			<Checkbox v-model="dontAsk" :label="formatMessage(trustMessages.trustDontAsk)" />
		</ConfirmModal>
		<Admonition v-if="modsQuery.isError.value" type="critical">
			{{ formatMessage(messages.loadError) }}
		</Admonition>
		<Admonition
			v-if="
				!appSettings.getFeatureFlag('hide_native_build_notice') &&
				installed.some((m) => m.kind === 'native-mod')
			"
			type="info"
		>
			{{ formatMessage(messages.nativeNotice) }}
		</Admonition>
		<ContentPageLayout />
		<section v-if="available.length > 0" class="flex flex-col gap-2">
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
	</div>
</template>
