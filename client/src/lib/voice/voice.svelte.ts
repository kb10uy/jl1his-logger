import { m } from '#lib/paraglide/messages.js';
import { toasts } from '#lib/state/toast.svelte.ts';
import { formatSpeech, formatTokens } from '#lib/voice/dsl/display.ts';
import { parseSpeech, type SpokenUpdate } from '#lib/voice/dsl/parser.ts';
import { AudioCapture } from '#lib/voice/speech/capture.ts';
import type { SpeechRecognizer } from '#lib/voice/speech/recognizer.ts';
import { sleep, withTimeout } from '#lib/voice/speech/timeout.ts';
import { VoskRecognizer } from '#lib/voice/speech/vosk.ts';
import {
    applyUpdates,
    carryOf,
    newDraft,
    setFreeText,
    type Carry,
    type Draft,
    type FreeTextField
} from './draft.ts';
import { MODEL_PATHS, WORKER_PATH, loadVoice, saveVoice, type VoiceSettings } from './settings.ts';

export type PttState = 'idle' | 'opening' | 'listening' | 'finishing';
export type EngineState = 'unloaded' | 'loading' | 'ready' | 'error';

export interface Utterance {
    id: number;
    at: Date;
    source: 'voice' | 'typed';
    heard: string;
    ok: boolean;
    message: string;
}

export interface Applied {
    source: Utterance['source'];
    ok: boolean;
    fields: SpokenUpdate['kind'][];
}

export type LogResult = { ok: true; call: string } | { ok: false; reason: string };

const FINAL_RESULT_TIMEOUT_MS = 10_000;
const RELEASE_TAIL_MS = 300;
const MAX_UTTERANCES = 30;

function vibrate(pattern: number | number[]) {
    try {
        navigator.vibrate?.(pattern);
    } catch {
        return;
    }
}

function errorMessage(e: unknown): string {
    return e instanceof Error ? e.message : String(e);
}

async function checkModel(url: string) {
    let response: Response;
    try {
        response = await fetch(url, { method: 'HEAD', credentials: 'same-origin' });
    } catch {
        return;
    }
    const type = response.headers.get('Content-Type') ?? '';
    if (!response.ok || type.startsWith('text/html'))
        throw new Error(m.voice_model_not_installed());
}

export class VoiceState {
    settings = $state<VoiceSettings>(loadVoice());
    engine = $state<EngineState>('unloaded');
    engineError = $state<string | undefined>(undefined);
    ptt = $state<PttState>('idle');
    partial = $state('');
    level = $state(0);
    draft: Draft;
    utterances = $state<Utterance[]>([]);

    #capture = new AudioCapture();
    #recognizer: SpeechRecognizer | null = null;
    #releaseRequested = false;
    #microphoneUsed = false;
    #wakeLock: WakeLockSentinel | null = null;
    #nextUtterance = 1;
    #load = 0;
    #listeners = new Set<(applied: Applied) => void>();

    constructor(
        carry: Carry,
        private readonly log: () => Promise<LogResult>
    ) {
        this.draft = $state(newDraft(carry));
    }

    get heard(): Utterance | undefined {
        return this.utterances[0];
    }

    start(): () => void {
        this.#capture.onAudio = (samples) => this.#recognizer?.pushAudio(samples);
        this.#capture.onLevel = (level) => (this.level = level);
        const visibility = () => this.#onVisibilityChange();
        document.addEventListener('visibilitychange', visibility);
        if (this.settings.autoLoad) void this.loadEngine();
        void this.#requestMicrophonePermission();
        return () => {
            document.removeEventListener('visibilitychange', visibility);
            void this.pttCancel();
            void this.#capture.close();
            this.#recognizer?.dispose();
            this.#recognizer = null;
            void this.#wakeLock?.release().catch(() => undefined);
            this.#wakeLock = null;
        };
    }

    async loadEngine() {
        if (this.engine === 'loading') return;
        this.#recognizer?.dispose();
        this.#recognizer = null;
        this.engine = 'loading';
        this.engineError = undefined;
        const load = ++this.#load;
        const language = this.settings.language;
        const modelUrl = new URL(MODEL_PATHS[language], location.href).href;
        const recognizer = new VoskRecognizer({
            modelUrl,
            workerUrl: new URL(WORKER_PATH, location.href).href,
            language
        });
        recognizer.onPartial = (text) => (this.partial = formatSpeech(text));
        try {
            await checkModel(modelUrl);
            await recognizer.initialize();
            if (load !== this.#load) {
                recognizer.dispose();
                return;
            }
            this.#recognizer = recognizer;
            this.engine = 'ready';
            if (!this.settings.autoLoad) this.#save({ ...this.settings, autoLoad: true });
        } catch (e) {
            recognizer.dispose();
            if (load !== this.#load) return;
            this.engine = 'error';
            this.engineError = errorMessage(e);
        }
    }

    #save(settings: VoiceSettings) {
        this.settings = settings;
        saveVoice(settings);
    }

    async pttPress() {
        if (this.ptt !== 'idle') return;
        const recognizer = this.#recognizer;
        if (recognizer === null || this.engine !== 'ready') {
            toasts.error(m.voice_engine_not_loaded_toast());
            return;
        }
        this.ptt = 'opening';
        this.#releaseRequested = false;
        this.partial = '';
        void this.#requestWakeLock();
        try {
            if (recognizer.needsAudio) await this.#capture.open();
        } catch (e) {
            this.ptt = 'idle';
            toasts.error(m.voice_microphone_error({ error: errorMessage(e) }));
            void this.#capture.close();
            return;
        }
        this.#microphoneUsed = true;
        if (this.#releaseRequested) {
            this.ptt = 'idle';
            toasts.show(m.voice_microphone_ready());
            return;
        }
        try {
            recognizer.beginUtterance();
            if (recognizer.needsAudio) this.#capture.begin();
        } catch (e) {
            this.ptt = 'idle';
            toasts.error(errorMessage(e));
            return;
        }
        this.ptt = 'listening';
        vibrate(15);
    }

    async pttRelease() {
        if (this.ptt === 'opening') {
            this.#releaseRequested = true;
            return;
        }
        if (this.ptt !== 'listening') return;
        const recognizer = this.#recognizer!;
        this.ptt = 'finishing';
        try {
            await sleep(RELEASE_TAIL_MS);
            if (recognizer.needsAudio) await this.#capture.end();
            const result = await withTimeout(
                recognizer.endUtterance(),
                FINAL_RESULT_TIMEOUT_MS,
                m.voice_recognition_timed_out()
            );
            await this.handleText(result.text, 'voice');
        } catch (e) {
            toasts.error(errorMessage(e));
            vibrate([60, 60, 60]);
        } finally {
            this.ptt = 'idle';
            this.partial = '';
            this.level = 0;
        }
    }

    async pttCancel() {
        if (this.ptt === 'opening') {
            this.#releaseRequested = true;
            return;
        }
        if (this.ptt !== 'listening') return;
        const recognizer = this.#recognizer!;
        this.ptt = 'finishing';
        try {
            if (recognizer.needsAudio) await this.#capture.end();
            recognizer.cancelUtterance();
        } finally {
            this.ptt = 'idle';
            this.partial = '';
            this.level = 0;
        }
    }

    async handleText(text: string, source: Utterance['source']) {
        const raw = text.trim();
        const parsed = parseSpeech(raw);
        const heard = formatTokens(parsed.tokens) || m.voice_nothing();
        let ok = parsed.ok;
        let message: string;
        if (parsed.ok && parsed.log) {
            const result = await this.log();
            ok = result.ok;
            message = result.ok ? m.voice_utterance_logged({ call: result.call }) : result.reason;
            vibrate(result.ok ? 40 : [60, 60, 60]);
        } else if (parsed.ok) {
            const { draft, descriptions } = applyUpdates(this.draft, parsed.updates);
            this.draft = draft;
            message = descriptions.join(m.voice_change_separator());
        } else {
            message = parsed.error;
            vibrate([60, 60, 60]);
        }
        const fields = parsed.ok ? parsed.updates.map((u) => u.kind) : [];
        for (const listener of this.#listeners) listener({ source, ok, fields });
        this.utterances = [
            { id: this.#nextUtterance++, at: new Date(), source, heard, ok, message },
            ...this.utterances
        ].slice(0, MAX_UTTERANCES);
    }

    onApplied(listener: (applied: Applied) => void): () => void {
        this.#listeners.add(listener);
        return () => this.#listeners.delete(listener);
    }

    setDraft(draft: Draft) {
        this.draft = draft;
    }

    setFreeText(field: FreeTextField, value: string) {
        this.draft = setFreeText(this.draft, field, value);
    }

    next(carry: Carry = carryOf(this.draft)) {
        this.draft = newDraft(carry);
    }

    async #requestMicrophonePermission() {
        try {
            const status = await navigator.permissions.query({
                name: 'microphone' as PermissionName
            });
            if (status.state !== 'prompt') return;
        } catch {
            return;
        }
        try {
            await this.#capture.requestPermission();
        } catch {
            return;
        }
    }

    async #requestWakeLock() {
        if (!this.settings.keepScreenOn || this.#wakeLock !== null || !('wakeLock' in navigator))
            return;
        try {
            this.#wakeLock = await navigator.wakeLock.request('screen');
            this.#wakeLock.addEventListener('release', () => (this.#wakeLock = null));
        } catch {
            return;
        }
    }

    #onVisibilityChange() {
        if (document.visibilityState === 'hidden') {
            void this.pttCancel();
            void this.#capture.close();
        } else {
            void this.#reopenMicrophone();
        }
    }

    async #reopenMicrophone() {
        if (!this.#microphoneUsed || this.engine !== 'ready') return;
        try {
            const status = await navigator.permissions.query({
                name: 'microphone' as PermissionName
            });
            if (status.state !== 'granted') return;
            await this.#capture.open();
        } catch {
            return;
        }
    }
}
