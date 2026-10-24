import { m } from '#lib/paraglide/messages.js';
import { grammarPhrases } from '#lib/voice/dsl/grammar.ts';
import type { SpeechLanguage } from '#lib/voice/dsl/lexicon.ts';
import { ASR_SAMPLE_RATE, type SpeechRecognizer, type SpeechResult } from './recognizer.ts';

export interface VoskWorker {
    postMessage(message: unknown, transfer: Transferable[]): void;
    addEventListener(type: 'message', listener: (event: MessageEvent) => void): void;
    terminate(): void;
}

export interface VoskOptions {
    modelUrl: string;
    workerUrl: string;
    language: SpeechLanguage;
    createWorker?: (url: string) => VoskWorker;
}

type Word = { text: string; confidence: number };

type VoskMessage =
    | { event: 'load'; result: boolean }
    | { event: 'error'; recognizerId?: string; error: string }
    | { event: 'partialresult'; recognizerId: string; result: { partial: string } }
    | {
          event: 'result';
          recognizerId: string;
          result: { text: string; result?: { word: string; conf: number }[] };
      };

const RECOGNIZER_ID = 'voice';
const INT16_SCALE = 0x8000;

export class VoskRecognizer implements SpeechRecognizer {
    readonly name = 'Vosk';
    readonly needsAudio = true;
    onPartial?: (text: string) => void;

    #worker: VoskWorker | null = null;
    #ready = false;
    #loading: { resolve: () => void; reject: (e: Error) => void } | null = null;
    #generation = 0;
    #inFlight: { generation: number; final: boolean }[] = [];
    #segments: string[] = [];
    #words: Word[] = [];
    #error: string | null = null;
    #finish: {
        generation: number;
        resolve: (r: SpeechResult) => void;
        reject: (e: Error) => void;
    } | null = null;

    constructor(readonly options: VoskOptions) {}

    async initialize(): Promise<void> {
        if (this.#worker !== null) return;
        const create: (url: string) => VoskWorker =
            this.options.createWorker ?? ((url) => new Worker(url));
        const worker = create(this.options.workerUrl);
        this.#worker = worker;
        worker.addEventListener('message', (event) => this.#receive(event.data));
        try {
            await new Promise<void>((resolve, reject) => {
                this.#loading = { resolve, reject };
                worker.postMessage({ action: 'set', key: 'logLevel', value: -1 }, []);
                worker.postMessage({ action: 'load', modelUrl: this.options.modelUrl }, []);
            });
        } catch (e) {
            this.dispose();
            throw e;
        }
        worker.postMessage(
            {
                action: 'create',
                recognizerId: RECOGNIZER_ID,
                sampleRate: ASR_SAMPLE_RATE,
                grammar: JSON.stringify(grammarPhrases(this.options.language))
            },
            []
        );
        worker.postMessage(
            {
                action: 'set',
                recognizerId: RECOGNIZER_ID,
                key: 'words',
                value: true
            },
            []
        );
        this.#ready = true;
    }

    #receive(message: VoskMessage | undefined) {
        if (message === undefined || message === null) return;
        const loading = this.#loading;
        if (loading !== null) {
            if (message.event === 'load') {
                this.#loading = null;
                if (message.result) loading.resolve();
                else loading.reject(new Error(m.voice_model_did_not_load()));
                return;
            }
            if (message.event === 'error' && message.recognizerId === undefined) {
                this.#loading = null;
                loading.reject(new Error(message.error));
                return;
            }
        }
        if (message.event === 'load' || message.recognizerId === undefined) return;
        this.#handle(message);
    }

    #handle(message: Exclude<VoskMessage, { event: 'load' }>) {
        const tag = this.#inFlight.shift();
        if (tag === undefined || tag.generation !== this.#generation) return;
        switch (message.event) {
            case 'partialresult':
                if (message.result.partial !== '') {
                    this.onPartial?.([...this.#segments, message.result.partial].join(' '));
                }
                break;
            case 'result':
                if (message.result.text !== '') this.#segments.push(message.result.text);
                for (const w of message.result.result ?? []) {
                    this.#words.push({ text: w.word, confidence: w.conf });
                }
                break;
            case 'error':
                this.#error ??= message.error;
                break;
        }
        const finish = this.#finish;
        if (tag.final && finish !== null && finish.generation === tag.generation) {
            this.#finish = null;
            if (this.#error !== null) finish.reject(new Error(this.#error));
            else finish.resolve({ text: this.#segments.join(' '), words: this.#words });
        }
    }

    #request(message: Record<string, unknown>, final: boolean, transfer: Transferable[] = []) {
        this.#inFlight.push({ generation: this.#generation, final });
        this.#worker!.postMessage({ ...message, recognizerId: RECOGNIZER_ID }, transfer);
    }

    beginUtterance(): void {
        this.#generation += 1;
        this.#segments = [];
        this.#words = [];
        this.#error = null;
    }

    pushAudio(samples: Float32Array): void {
        if (!this.#ready) return;
        const data = samples.map((value) => value * INT16_SCALE);
        this.#request({ action: 'audioChunk', data, sampleRate: ASR_SAMPLE_RATE }, false, [
            data.buffer
        ]);
    }

    endUtterance(): Promise<SpeechResult> {
        if (!this.#ready) return Promise.reject(new Error('Vosk is not initialized'));
        return new Promise((resolve, reject) => {
            this.#finish = { generation: this.#generation, resolve, reject };
            this.#request({ action: 'retrieveFinalResult' }, true);
        });
    }

    cancelUtterance(): void {
        if (!this.#ready) return;
        this.#request({ action: 'retrieveFinalResult' }, true);
        this.#finish?.reject(new Error('cancelled'));
        this.#finish = null;
        this.#generation += 1;
    }

    dispose(): void {
        this.#worker?.terminate();
        this.#worker = null;
        this.#ready = false;
        this.#loading = null;
        this.#inFlight = [];
    }
}
