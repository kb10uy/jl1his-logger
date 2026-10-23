export interface SpeechResult {
    text: string;
    words?: {
        text: string;
        confidence: number;
    }[];
}

export interface SpeechRecognizer {
    readonly name: string;
    readonly needsAudio: boolean;

    initialize(): Promise<void>;
    beginUtterance(): void;
    pushAudio(samples: Float32Array): void;
    endUtterance(): Promise<SpeechResult>;
    cancelUtterance(): void;

    onPartial?: (text: string) => void;

    dispose(): void;
}

export const ASR_SAMPLE_RATE = 16_000;
