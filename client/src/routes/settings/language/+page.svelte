<script lang="ts">
    import { chooseLocale, LOCALE_NAMES, locales, storedLocale, type Locale } from '#lib/i18n.ts';
    import { m } from '#lib/paraglide/messages.js';

    const chosen = storedLocale();
</script>

<section aria-label={m.settings_language()} class="flex max-w-2xl flex-col gap-4">
    <div class="flex flex-col gap-1">
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
