<script setup lang="ts">
import { TrashIcon } from '@modrinth/assets'
import {
	Button,
	Combobox,
	type ComboboxOption,
	defineMessages,
	IconButton,
	injectNotificationManager,
	useVIntl,
} from '@modrinth/ui'
import { useMutation, useQuery, useQueryClient } from '@tanstack/vue-query'
import { open } from '@tauri-apps/plugin-dialog'
import { computed, ref } from 'vue'

import {
	apply_game_options_profile,
	delete_game_options_profile,
	type GameOptionsProfile,
	guess_game_options_profile_version,
	import_game_options_profile,
	list_game_options_profiles,
	save_game_options_profile,
} from '@/helpers/game-options'
import { instanceListQueryOptions } from '@/pages/instance/query-options'

const emit = defineEmits<{ applied: [] }>()

const { handleError, addNotification } = injectNotificationManager()
const { formatMessage } = useVIntl()
const queryClient = useQueryClient()

const messages = defineMessages({
	title: { id: 'app.settings.options-profiles.title', defaultMessage: 'Options profiles' },
	description: {
		id: 'app.settings.options-profiles.description',
		defaultMessage:
			'Save your options and key bindings, then apply them to every instance that syncs its game settings. Each setting is converted for each instance’s Minecraft version; settings a version does not have are left out.',
	},
	empty: {
		id: 'app.settings.options-profiles.empty',
		defaultMessage: 'No profiles yet.',
	},
	details: {
		id: 'app.settings.options-profiles.details',
		defaultMessage: 'Minecraft {version} · {settings} settings',
	},
	apply: { id: 'app.settings.options-profiles.apply', defaultMessage: 'Apply to instances' },
	applied: {
		id: 'app.settings.options-profiles.applied',
		defaultMessage:
			'Applied to {applied} instances. {deferred, plural, =0 {} other {# will get it when they close.}}',
	},
	delete: { id: 'app.settings.options-profiles.delete', defaultMessage: 'Delete profile' },
	name: { id: 'app.settings.options-profiles.name', defaultMessage: 'Profile name' },
	fromInstance: {
		id: 'app.settings.options-profiles.from-instance',
		defaultMessage: 'From instance',
	},
	save: {
		id: 'app.settings.options-profiles.save',
		defaultMessage: 'Save from instance',
	},
	importFile: {
		id: 'app.settings.options-profiles.import',
		defaultMessage: 'Import options.txt…',
	},
	version: {
		id: 'app.settings.options-profiles.version',
		defaultMessage: 'Minecraft version of this file',
	},
	confirmImport: {
		id: 'app.settings.options-profiles.confirm-import',
		defaultMessage: 'Import',
	},
})

const profilesKey = ['game-options-profiles'] as const
const profilesQuery = useQuery({
	queryKey: profilesKey,
	networkMode: 'always',
	queryFn: list_game_options_profiles,
})
const profiles = computed(() => profilesQuery.data.value ?? [])

const instancesQuery = useQuery(instanceListQueryOptions())
const instanceOptions = computed<ComboboxOption<string>[]>(() =>
	(instancesQuery.data.value ?? []).map((i) => ({ value: i.id, label: i.name })),
)

const name = ref('')
const instanceId = ref('')
const importPath = ref<string | null>(null)
const importVersion = ref('')

async function refresh() {
	await queryClient.invalidateQueries({ queryKey: profilesKey })
}

const saveMutation = useMutation({
	networkMode: 'always',
	mutationFn: () => save_game_options_profile(name.value, instanceId.value),
	onSuccess: async () => {
		name.value = ''
		await refresh()
	},
	onError: handleError,
})

const importMutation = useMutation({
	networkMode: 'always',
	mutationFn: () =>
		import_game_options_profile(name.value, importPath.value ?? '', importVersion.value),
	onSuccess: async () => {
		name.value = ''
		importPath.value = null
		await refresh()
	},
	onError: handleError,
})

const applyMutation = useMutation({
	networkMode: 'always',
	mutationFn: (profile: GameOptionsProfile) => apply_game_options_profile(profile.id),
	onSuccess: (result, profile) => {
		addNotification({
			type: 'success',
			title: profile.name,
			text: formatMessage(messages.applied, {
				applied: result.applied + result.migrated,
				deferred: result.deferred,
			}),
		})
		emit('applied')
	},
	onError: handleError,
})

const deleteMutation = useMutation({
	networkMode: 'always',
	mutationFn: (profile: GameOptionsProfile) => delete_game_options_profile(profile.id),
	onSuccess: refresh,
	onError: handleError,
})

const busy = computed(
	() =>
		saveMutation.isPending.value ||
		importMutation.isPending.value ||
		applyMutation.isPending.value ||
		deleteMutation.isPending.value,
)

async function pickFile() {
	const picked = await open({
		multiple: false,
		filters: [{ name: 'options.txt', extensions: ['txt'] }],
	})
	if (typeof picked !== 'string') return
	importPath.value = picked
	importVersion.value = (await guess_game_options_profile_version(picked).catch(() => null)) ?? ''
}
</script>

<template>
	<section class="flex flex-col gap-4">
		<div class="flex flex-col gap-1">
			<h2 class="m-0 text-lg font-semibold text-contrast">{{ formatMessage(messages.title) }}</h2>
			<p class="m-0 text-secondary">{{ formatMessage(messages.description) }}</p>
		</div>

		<p v-if="profiles.length === 0" class="m-0 text-secondary">
			{{ formatMessage(messages.empty) }}
		</p>
		<div
			v-for="profile in profiles"
			:key="profile.id"
			class="flex items-center justify-between gap-4"
			data-options-profile
		>
			<div class="flex min-w-0 flex-col">
				<span class="truncate font-semibold text-contrast">{{ profile.name }}</span>
				<span class="text-sm text-secondary">
					{{
						formatMessage(messages.details, {
							version: profile.game_version,
							settings: profile.settings,
						})
					}}
				</span>
			</div>
			<div class="flex shrink-0 items-center gap-2">
				<Button :disabled="busy" @click="applyMutation.mutate(profile)">
					{{ formatMessage(messages.apply) }}
				</Button>
				<IconButton
					type="outlined"
					circular
					:disabled="busy"
					:label="formatMessage(messages.delete)"
					@click="deleteMutation.mutate(profile)"
				>
					<TrashIcon aria-hidden="true" />
				</IconButton>
			</div>
		</div>

		<label class="flex flex-col gap-1">
			{{ formatMessage(messages.name) }}
			<input v-model="name" maxlength="48" data-options-profile-name />
		</label>
		<div v-if="importPath" class="flex flex-col gap-2">
			<span class="break-all text-sm text-secondary">{{ importPath }}</span>
			<label class="flex flex-col gap-1">
				{{ formatMessage(messages.version) }}
				<input v-model="importVersion" maxlength="16" placeholder="1.21.11" />
			</label>
			<Button
				type="colored"
				color="brand"
				:disabled="busy || !name.trim() || !importVersion.trim()"
				@click="importMutation.mutate()"
			>
				{{ formatMessage(messages.confirmImport) }}
			</Button>
		</div>
		<div v-else class="flex flex-wrap items-end gap-2">
			<div class="flex min-w-48 flex-1 flex-col gap-1">
				<span>{{ formatMessage(messages.fromInstance) }}</span>
				<Combobox v-model="instanceId" :options="instanceOptions" searchable />
			</div>
			<Button :disabled="busy || !name.trim() || !instanceId" @click="saveMutation.mutate()">
				{{ formatMessage(messages.save) }}
			</Button>
			<Button :disabled="busy" @click="pickFile">
				{{ formatMessage(messages.importFile) }}
			</Button>
		</div>
	</section>
</template>
