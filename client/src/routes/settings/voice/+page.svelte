<script lang="ts">
    import SettingsField from '#lib/components/SettingsField.svelte';
    import SettingsSection from '#lib/components/SettingsSection.svelte';
    import { m } from '#lib/paraglide/messages.js';
    import type { SpeechLanguage } from '#lib/voice/dsl/lexicon.ts';
    import { LANGUAGE_LABELS, loadVoice, saveVoice } from '#lib/voice/settings.ts';

    let language = $state(loadVoice().language);

    function setLanguage(value: SpeechLanguage) {
        language = value;
        saveVoice({ ...loadVoice(), language });
    }
</script>

<SettingsSection title={m.settings_voice_recognition()}>
    <div class="grid grid-cols-1 gap-x-4 gap-y-3 md:grid-cols-[160px_minmax(0,1fr)]">
        <SettingsField
            label={m.voice_speech_language()}
            for="voice-language"
            hint={m.voice_speech_language_hint()}
        >
            <select
                id="voice-language"
                class="control h-10 w-full md:w-60"
                value={language}
                onchange={(e) => setLanguage(e.currentTarget.value as SpeechLanguage)}
            >
                {#each Object.entries(LANGUAGE_LABELS) as [value, label] (value)}
                    <option {value}>{label()}</option>
                {/each}
            </select>
        </SettingsField>
    </div>
</SettingsSection>
