<script lang="ts">
    import { m } from '#lib/paraglide/messages.js';
    import type { SpeechLanguage } from '#lib/voice/dsl/lexicon.ts';
    import { LANGUAGE_LABELS, loadVoice, saveVoice } from '#lib/voice/settings.ts';

    let language = $state(loadVoice().language);

    function setLanguage(value: SpeechLanguage) {
        language = value;
        saveVoice({ ...loadVoice(), language });
    }
</script>

<section aria-label={m.settings_voice()} class="flex max-w-2xl flex-col gap-4">
    <div class="flex flex-col gap-1">
        <label for="voice-language" class="text-xs text-muted">{m.voice_speech_language()}</label>
        <select
            id="voice-language"
            class="control w-52"
            value={language}
            onchange={(e) => setLanguage(e.currentTarget.value as SpeechLanguage)}
        >
            {#each Object.entries(LANGUAGE_LABELS) as [value, label] (value)}
                <option {value}>{label()}</option>
            {/each}
        </select>
        <span class="text-xs text-muted">{m.voice_speech_language_hint()}</span>
    </div>
</section>
