import { useAppSettings } from '@/composables/use-app-settings.ts'
import { get as getSettings, set as setSettings } from '@/helpers/settings.ts'

/** Whether a native mod installs without the trust question: the player turned it off themself. */
export const nativeTrustSkipped = () => useAppSettings().getFeatureFlag('skip_native_mod_trust')

/** The trust dialog's "Don't ask again", kept with the app's settings (Settings > Behavior undoes it). */
export async function skipNativeTrust() {
	useAppSettings().featureFlags.skip_native_mod_trust = true
	const settings = await getSettings()
	settings.feature_flags = { ...settings.feature_flags, skip_native_mod_trust: true }
	await setSettings(settings)
}
