<script lang="ts">
    import type { SpeechLanguage } from '#lib/voice/dsl/lexicon.ts';
    import { LANGUAGE_LABELS, loadVoice, saveVoice } from '#lib/voice/settings.ts';

    let language = $state(loadVoice().language);

    function setLanguage(value: SpeechLanguage) {
        language = value;
        saveVoice({ ...loadVoice(), language });
    }
</script>

<section aria-label="Voice entry" class="flex max-w-2xl flex-col gap-4">
    <div class="flex flex-col gap-1">
        <label for="voice-language" class="text-xs text-muted">Speech language</label>
        <select
            id="voice-language"
            class="control w-52"
            value={language}
            onchange={(e) => setLanguage(e.currentTarget.value as SpeechLanguage)}
        >
            {#each Object.entries(LANGUAGE_LABELS) as [value, label] (value)}
                <option {value}>{label}</option>
            {/each}
        </select>
        <span class="text-xs text-muted"
            >Voice entry loads the model of this language. Each model is downloaded once and then
            kept on this device.</span
        >
    </div>
</section>
