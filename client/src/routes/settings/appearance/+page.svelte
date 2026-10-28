<script lang="ts">
    import { chooseLocale, LOCALE_NAMES, locales, storedLocale, type Locale } from '#lib/i18n.ts';
    import { m } from '#lib/paraglide/messages.js';
    import { asTheme, chooseTheme, storedTheme } from '#lib/theme.ts';

    const chosen = storedLocale();
    const themes = [
        { value: 'light', label: m.theme_light() },
        { value: 'dark', label: m.theme_dark() }
    ];
</script>

<section aria-label={m.settings_appearance()} class="flex flex-col gap-4">
    <div class="flex flex-col gap-1 rounded-lg border border-border bg-surface p-4">
        <label for="theme" class="text-xs text-muted">{m.theme_label()}</label>
        <select
            id="theme"
            class="control w-52"
            value={storedTheme() ?? ''}
            onchange={(e) => chooseTheme(asTheme(e.currentTarget.value))}
        >
            <option value="">{m.theme_system()}</option>
            {#each themes as theme (theme.value)}
                <option value={theme.value}>{theme.label}</option>
            {/each}
        </select>
    </div>
    <div class="flex flex-col gap-1 rounded-lg border border-border bg-surface p-4">
        <label for="display-language" class="text-xs text-muted">{m.language_label()}</label>
        <select
            id="display-language"
            class="control w-52"
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
        <span class="text-xs text-muted">{m.language_hint()}</span>
    </div>
</section>
