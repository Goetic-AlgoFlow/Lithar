import { defineMessages } from '@modrinth/ui'

// Asked before a native Oxyra mod installs, on the instance's page and in Discover.
export const trustMessages = defineMessages({
	trustTitle: {
		id: 'app.horizon-mods.trust.title',
		defaultMessage: 'Trust this native mod?',
	},
	trustDescription: {
		id: 'app.horizon-mods.trust.description',
		defaultMessage:
			"**{name}** is native code: it is not sandboxed and runs with full access to this computer. Oxyra's registry reviewed it before listing it, but read its source if you are unsure. It will be compiled into this instance's game before its next launch.",
	},
	trustProceed: {
		id: 'app.horizon-mods.trust.proceed',
		defaultMessage: 'I trust this mod: install',
	},
	trustDontAsk: {
		id: 'app.horizon-mods.trust.dont-ask',
		defaultMessage: "Don't ask again for native mods (Settings > Behavior turns it back on)",
	},
})
