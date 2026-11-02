<script setup lang="ts">
import { ExternalIcon } from '@modrinth/assets'
import { defineMessages, useVIntl } from '@modrinth/ui'

import LitharSettingsPage from '@/components/ui/settings/lithar/LitharSettingsPage.vue'

const { formatMessage } = useVIntl()

const messages = defineMessages({
	title: {
		id: 'lithar.app.settings.credits.title',
		defaultMessage: 'Credits',
	},
	description: {
		id: 'lithar.app.settings.credits.description',
		defaultMessage: 'Lithar is built on the work of these open-source projects.',
	},
	modrinth: {
		id: 'lithar.app.settings.credits.modrinth',
		defaultMessage: 'The launcher Lithar is built from.',
	},
	astralrinth: {
		id: 'lithar.app.settings.credits.astralrinth',
		defaultMessage: 'The fork Lithar continues: external accounts, skins and visual settings.',
	},
	horizon: {
		id: 'lithar.app.settings.credits.horizon',
		defaultMessage: 'Minecraft: Java Edition rewritten in Rust, the base of Oxyra.',
	},
	license: {
		id: 'lithar.app.settings.credits.license',
		defaultMessage: 'License: {license}',
	},
	notice: {
		id: 'lithar.app.settings.credits.notice',
		defaultMessage:
			'Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft.',
	},
})

const projects = [
	{
		name: 'Modrinth App',
		description: messages.modrinth,
		license: 'GPL-3.0',
		url: 'https://github.com/modrinth/code',
	},
	{
		name: 'AstralRinth',
		description: messages.astralrinth,
		license: 'GPL-3.0',
		url: 'https://git.xorison.dev/didirus/AstralRinth',
	},
	{
		name: 'Horizon',
		description: messages.horizon,
		license: 'MIT',
		url: 'https://github.com/horizon-rust/horizon',
	},
]
</script>

<template>
	<LitharSettingsPage
		:title="formatMessage(messages.title)"
		:description="formatMessage(messages.description)"
	>
		<div class="flex flex-col gap-3">
			<a
				v-for="project in projects"
				:key="project.name"
				:href="project.url"
				target="_blank"
				rel="noopener noreferrer"
				class="flex items-start justify-between gap-4 rounded-2xl border border-solid border-surface-5 bg-surface-3 p-4 text-primary no-underline hover:brightness-110"
			>
				<div class="min-w-0">
					<p class="m-0 font-semibold text-contrast">{{ project.name }}</p>
					<p class="m-0 mt-1 text-sm text-secondary">
						{{ formatMessage(project.description) }}
					</p>
					<p class="m-0 mt-1 text-xs text-secondary">
						{{ formatMessage(messages.license, { license: project.license }) }}
					</p>
				</div>
				<ExternalIcon class="size-4 shrink-0 text-secondary" aria-hidden="true" />
			</a>
		</div>
		<template #footer>
			{{ formatMessage(messages.notice) }}
		</template>
	</LitharSettingsPage>
</template>
