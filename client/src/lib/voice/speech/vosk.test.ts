import { describe, expect, it } from 'vitest';
import { VoskRecognizer, type VoskWorker } from './vosk.ts';

type Posted = { action: string; [key: string]: unknown };

class FakeWorker implements VoskWorker {
    posted: Posted[] = [];
    terminated = false;
    private listener: ((event: MessageEvent) => void) | undefined;

    constructor(readonly url: string) {}

    postMessage(message: unknown) {
        this.posted.push(message as Posted);
    }

    addEventListener(_type: 'message', listener: (event: MessageEvent) => void) {
        this.listener = listener;
    }

    terminate() {
        this.terminated = true;
    }

    send(data: unknown) {
        this.listener?.({ data } as MessageEvent);
    }

    get requests(): string[] {
        return this.posted
            .filter((m) => m.action === 'audioChunk' || m.action === 'retrieveFinalResult')
            .map((m) =>
                m.action === 'audioChunk' ? `audio:${(m.data as Float32Array).length}` : 'final'
            );
    }

    partial(text: string) {
        this.send({ event: 'partialresult', recognizerId: 'voice', result: { partial: text } });
    }

    result(text: string) {
        const words =
            text === '' ? [] : text.split(' ').map((word) => ({ word, conf: 1, start: 0, end: 0 }));
        this.send({ event: 'result', recognizerId: 'voice', result: { text, result: words } });
    }

    error(error: string) {
        this.send({ event: 'error', recognizerId: 'voice', error });
    }
}

async function recognizer(language: 'en' | 'ja' = 'en') {
    let fake: FakeWorker | undefined;
    const r = new VoskRecognizer({
        modelUrl: 'https://example.com/model.tar.gz',
        workerUrl: 'https://example.com/vosk-worker.js',
        language,
        createWorker: (url) => (fake = new FakeWorker(url))
    });
    const loading = r.initialize();
    fake!.send({ event: 'load', result: true });
    await loading;
    return { r, fake: fake! };
}

const chunk = () => new Float32Array(1600);

describe('VoskRecognizer', () => {
    it('loads the model and creates a 16 kHz recognizer with the DSL grammar', async () => {
        const { fake } = await recognizer();
        expect(fake.url).toBe('https://example.com/vosk-worker.js');
        expect(fake.posted.find((m) => m.action === 'load')).toMatchObject({
            modelUrl: 'https://example.com/model.tar.gz'
        });
        const create = fake.posted.find((m) => m.action === 'create')!;
        expect(create.sampleRate).toBe(16_000);
        const grammar: string[] = JSON.parse(create.grammar as string);
        expect(grammar).toContain('[unk]');
        expect(grammar.some((phrase) => phrase.startsWith('received '))).toBe(true);
    });

    it('spells the grammar in Japanese for a Japanese model', async () => {
        const { fake } = await recognizer('ja');
        const create = fake.posted.find((m) => m.action === 'create')!;
        const grammar: string[] = JSON.parse(create.grammar as string);
        expect(grammar).toContain('[unk]');
        expect(grammar.some((phrase) => phrase.startsWith('受信 '))).toBe(true);
        expect(grammar.some((phrase) => phrase.startsWith('received '))).toBe(false);
    });

    it('fails to initialize when the worker cannot load the model', async () => {
        let fake: FakeWorker | undefined;
        const r = new VoskRecognizer({
            modelUrl: 'https://example.com/missing.tar.gz',
            workerUrl: 'https://example.com/vosk-worker.js',
            language: 'en',
            createWorker: (url) => (fake = new FakeWorker(url))
        });
        const loading = r.initialize();
        fake!.send({ event: 'error', error: 'Failed to fetch' });
        await expect(loading).rejects.toThrow('Failed to fetch');
        expect(fake!.terminated).toBe(true);
    });

    it('scales samples to the 16-bit range Kaldi expects', async () => {
        const { r, fake } = await recognizer();
        r.beginUtterance();
        r.pushAudio(Float32Array.from([0.5, -1]));
        const audio = fake.posted.find((m) => m.action === 'audioChunk')!;
        expect(Array.from(audio.data as Float32Array)).toEqual([16384, -32768]);
    });

    it('joins segments finalised mid-utterance with the final result', async () => {
        const { r, fake } = await recognizer();
        const partials: string[] = [];
        r.onPartial = (t) => partials.push(t);
        r.beginUtterance();
        r.pushAudio(chunk());
        r.pushAudio(chunk());
        r.pushAudio(chunk());
        const done = r.endUtterance();
        expect(fake.requests).toEqual(['audio:1600', 'audio:1600', 'audio:1600', 'final']);
        fake.partial('received');
        fake.result('received');
        fake.partial('five');
        fake.result('five seven');
        await expect(done).resolves.toMatchObject({ text: 'received five seven' });
        expect(partials).toEqual(['received', 'received five']);
    });

    it('waits for responses to every chunk before resolving', async () => {
        const { r, fake } = await recognizer();
        r.beginUtterance();
        r.pushAudio(chunk());
        r.pushAudio(chunk());
        let resolved = false;
        const done = r.endUtterance().then((res) => ((resolved = true), res));
        fake.partial('sent');
        await Promise.resolve();
        expect(resolved).toBe(false);
        fake.partial('sent five');
        fake.result('sent five nine');
        await expect(done).resolves.toMatchObject({ text: 'sent five nine' });
    });

    it('drops late responses of a cancelled utterance', async () => {
        const { r, fake } = await recognizer();
        r.beginUtterance();
        r.pushAudio(chunk());
        r.cancelUtterance();
        r.beginUtterance();
        r.pushAudio(chunk());
        const done = r.endUtterance();
        fake.partial('juliett');
        fake.result('juliett lima');
        fake.partial('card');
        fake.result('card requested');
        await expect(done).resolves.toMatchObject({ text: 'card requested' });
    });

    it('rejects when the worker reports an error', async () => {
        const { r, fake } = await recognizer();
        r.beginUtterance();
        r.pushAudio(chunk());
        const done = r.endUtterance();
        fake.error('Recognizer not ready');
        fake.result('');
        await expect(done).rejects.toThrow('Recognizer not ready');
    });

    it('ignores the replies the worker sends to configuration', async () => {
        const { r, fake } = await recognizer();
        r.beginUtterance();
        r.pushAudio(chunk());
        const done = r.endUtterance();
        fake.send(undefined);
        fake.partial('qso');
        fake.result('qso');
        await expect(done).resolves.toMatchObject({ text: 'qso' });
    });
});
