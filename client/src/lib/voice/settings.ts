import { m } from '#lib/paraglide/messages.js';
import type { SpeechLanguage } from '#lib/voice/dsl/lexicon.ts';

export interface VoiceSettings {
    language: SpeechLanguage;
    autoLoad: boolean;
    keepScreenOn: boolean;
}

export const MODEL_PATHS: Readonly<Record<SpeechLanguage, string>> = {
    en: '/models/vosk-model-small-en-us-0.15.tar.gz',
    ja: '/models/vosk-model-small-ja-0.22.tar.gz'
};

export const WORKER_PATH = '/vosk-worker.js';

export const LANGUAGE_LABELS: Readonly<Record<SpeechLanguage, () => string>> = {
    en: () => m.voice_language_en(),
    ja: () => m.voice_language_ja()
};

export function speechLanguageFor(browserLanguage: string | undefined): SpeechLanguage {
    return browserLanguage?.toLowerCase().split('-')[0] === 'ja' ? 'ja' : 'en';
}

export function defaultVoice(browserLanguage?: string): VoiceSettings {
    return { language: speechLanguageFor(browserLanguage), autoLoad: false, keepScreenOn: true };
}
