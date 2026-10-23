import workletUrl from './pcm-worklet.ts?worker&url';
import type { WorkletCommand, WorkletEvent } from './pcm-worklet.ts';
import { withTimeout } from './timeout.ts';

const RESUME_TIMEOUT_MS = 5_000;
const MICROPHONE_TIMEOUT_MS = 30_000;
const WORKLET_TIMEOUT_MS = 10_000;
const STOP_TIMEOUT_MS = 500;

function stateOf(context: AudioContext): AudioContextState {
    return context.state;
}

export class AudioCapture {
    #context: AudioContext | null = null;
    #stream: MediaStream | null = null;
    #node: AudioWorkletNode | null = null;
    #source: MediaStreamAudioSourceNode | null = null;
    #stopped: (() => void) | null = null;
    #starting: Promise<void> | null = null;

    onAudio: (samples: Float32Array) => void = () => {};
    onLevel: (level: number) => void = () => {};

    get isOpen(): boolean {
        return this.#node !== null;
    }

    async requestPermission(): Promise<void> {
        if (this.#node !== null || this.#starting !== null) return;
        const stream = await this.#requestMicrophone();
        stream.getTracks().forEach((t) => t.stop());
    }

    open(): Promise<void> {
        if (this.#node !== null) return this.#resume(this.#context!);
        this.#starting ??= this.#open().finally(() => (this.#starting = null));
        return this.#starting;
    }

    async #resume(context: AudioContext): Promise<void> {
        if (context.state === 'running') return;
        await withTimeout(context.resume(), RESUME_TIMEOUT_MS, 'audio did not start');
        const state = stateOf(context);
        if (state !== 'running') throw new Error(`audio is ${state}`);
    }

    async #open(): Promise<void> {
        const context = new AudioContext({ latencyHint: 'interactive' });
        const resumed = context.resume();
        let stream: MediaStream | null = null;
        try {
            stream = await this.#requestMicrophone();
            await withTimeout(resumed, RESUME_TIMEOUT_MS, 'audio did not start');
            await this.#resume(context);
            await withTimeout(
                context.audioWorklet.addModule(workletUrl),
                WORKLET_TIMEOUT_MS,
                'the audio processor did not load'
            );
            const source = context.createMediaStreamSource(stream);
            const node = new AudioWorkletNode(context, 'pcm-processor', {
                numberOfInputs: 1,
                numberOfOutputs: 0
            });
            node.port.onmessage = (event: MessageEvent<WorkletEvent>) => this.#handle(event.data);
            source.connect(node);

            this.#context = context;
            this.#stream = stream;
            this.#source = source;
            this.#node = node;
        } catch (e) {
            stream?.getTracks().forEach((t) => t.stop());
            await context.close().catch(() => {});
            throw e;
        }
    }

    async #requestMicrophone(): Promise<MediaStream> {
        const request = navigator.mediaDevices.getUserMedia({
            audio: {
                channelCount: 1,
                echoCancellation: true,
                noiseSuppression: true,
                autoGainControl: true
            }
        });
        try {
            return await withTimeout(
                request,
                MICROPHONE_TIMEOUT_MS,
                'the microphone did not respond'
            );
        } catch (e) {
            request.then(
                (stream) => stream.getTracks().forEach((t) => t.stop()),
                () => {}
            );
            throw e;
        }
    }

    #handle(event: WorkletEvent) {
        switch (event.type) {
            case 'audio':
                this.onLevel(event.level);
                this.onAudio(event.samples);
                break;
            case 'stopped':
                this.#stopped?.();
                break;
        }
    }

    #send(command: WorkletCommand) {
        this.#node?.port.postMessage(command);
    }

    begin() {
        if (this.#node === null) throw new Error('microphone is not open');
        this.#send({ type: 'start' });
    }

    end(): Promise<void> {
        if (this.#node === null) return Promise.resolve();
        return new Promise((resolve) => {
            const previous = this.#stopped;
            const stopped = () => {
                if (this.#stopped === stopped) this.#stopped = null;
                clearTimeout(timer);
                previous?.();
                resolve();
            };
            const timer = setTimeout(stopped, STOP_TIMEOUT_MS);
            this.#stopped = stopped;
            this.#send({ type: 'stop' });
        });
    }

    async close() {
        if (this.#starting !== null) await this.#starting.catch(() => {});
        const { context, stream, source, node } = {
            context: this.#context,
            stream: this.#stream,
            source: this.#source,
            node: this.#node
        };
        this.#context = null;
        this.#stream = null;
        this.#source = null;
        this.#node = null;
        this.#stopped?.();
        source?.disconnect();
        node?.disconnect();
        stream?.getTracks().forEach((t) => t.stop());
        await context?.close().catch(() => {});
    }
}
