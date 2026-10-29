<script lang="ts">
    import SettingsField from '#lib/components/SettingsField.svelte';
    import SettingsSection from '#lib/components/SettingsSection.svelte';
    import { chooseLocale, LOCALE_NAMES, locales, storedLocale, type Locale } from '#lib/i18n.ts';
    import { chooseCompact, storedCompact } from '#lib/compact.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { asTheme, chooseTheme, storedTheme } from '#lib/theme.ts';

    const chosen = storedLocale();
    const themes = [
        { value: 'light', label: m.theme_light() },
        { value: 'dark', label: m.theme_dark() }
    ];
</script>

<SettingsSection title={m.settings_appearance_display()}>
    <div class="grid grid-cols-1 gap-x-4 gap-y-3 md:grid-cols-[160px_minmax(0,1fr)]">
        <SettingsField label={m.theme_label()} for="theme">
            <select
                id="theme"
                class="control h-10 w-full md:w-60"
                value={storedTheme() ?? ''}
                onchange={(e) => chooseTheme(asTheme(e.currentTarget.value))}
            >
                <option value="">{m.theme_system()}</option>
                {#each themes as theme (theme.value)}
                    <option value={theme.value}>{theme.label}</option>
                {/each}
            </select>
        </SettingsField>
        <SettingsField label={m.density_label()} for="density" hint={m.density_hint()}>
            <select
                id="density"
                class="control h-10 w-full md:w-60"
                value={storedCompact() ? 'compact' : ''}
                onchange={(e) => chooseCompact(e.currentTarget.value === 'compact')}
            >
                <option value="">{m.density_normal()}</option>
                <option value="compact">{m.density_compact()}</option>
            </select>
        </SettingsField>
        <SettingsField label={m.language_label()} for="display-language" hint={m.language_hint()}>
            <select
                id="display-language"
                class="control h-10 w-full md:w-60"
                value={chosen ?? ''}
                onchange={(e) => {
                    const value = e.currentTarget.value;
                    chooseLocale(value === '' ? null : (value as Locale));
                }}
            >
                <option value="">{m.language_browser()}</option>
                {#each locales as locale (locale)}
                    <option value={locale}>{LOCALE_NAMES[locale]}</option>
                {/each}
            </select>
        </SettingsField>
    </div>
</SettingsSection>
