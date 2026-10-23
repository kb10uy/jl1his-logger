export class StreamingResampler {
    readonly #step: number;
    readonly #taps: Float32Array | null;
    readonly #history: Float32Array;
    #historyPos = 0;
    #prev = 0;
    #pos = 0;
    #primed = false;

    constructor(
        readonly inputRate: number,
        readonly outputRate: number,
        tapCount = 31
    ) {
        if (!(inputRate > 0) || !(outputRate > 0))
            throw new RangeError('sample rates must be positive');
        this.#step = inputRate / outputRate;
        this.#taps =
            inputRate > outputRate
                ? designLowPass(tapCount, (0.45 * outputRate) / inputRate)
                : null;
        this.#history = new Float32Array(this.#taps?.length ?? 1);
    }

    #filter(sample: number): number {
        const taps = this.#taps;
        if (taps === null) return sample;
        const n = taps.length;
        this.#history[this.#historyPos] = sample;
        this.#historyPos = (this.#historyPos + 1) % n;
        let acc = 0;
        let idx = this.#historyPos;
        for (let k = 0; k < n; k++) {
            acc += taps[k] * this.#history[idx];
            idx = idx + 1 === n ? 0 : idx + 1;
        }
        return acc;
    }

    process(input: Float32Array): Float32Array {
        if (this.inputRate === this.outputRate) return input.slice();
        const out: number[] = [];
        for (let i = 0; i < input.length; i++) {
            const current = this.#filter(input[i]);
            if (!this.#primed) {
                this.#prev = current;
                this.#primed = true;
                continue;
            }
            while (this.#pos < 1) {
                out.push(this.#prev + (current - this.#prev) * this.#pos);
                this.#pos += this.#step;
            }
            this.#pos -= 1;
            this.#prev = current;
        }
        return Float32Array.from(out);
    }
}

function designLowPass(tapCount: number, cutoff: number): Float32Array {
    const taps = new Float32Array(tapCount);
    const mid = (tapCount - 1) / 2;
    let sum = 0;
    for (let i = 0; i < tapCount; i++) {
        const x = i - mid;
        const sinc = x === 0 ? 2 * cutoff : Math.sin(2 * Math.PI * cutoff * x) / (Math.PI * x);
        const window = 0.5 - 0.5 * Math.cos((2 * Math.PI * i) / (tapCount - 1));
        taps[i] = sinc * window;
        sum += taps[i];
    }
    for (let i = 0; i < tapCount; i++) taps[i] /= sum;
    return taps;
}

export function downmix(channels: readonly Float32Array[]): Float32Array {
    if (channels.length === 1) return channels[0];
    const length = channels[0]?.length ?? 0;
    const out = new Float32Array(length);
    for (const channel of channels) {
        for (let i = 0; i < length; i++) out[i] += channel[i];
    }
    for (let i = 0; i < length; i++) out[i] /= channels.length;
    return out;
}

export function rms(samples: Float32Array): number {
    if (samples.length === 0) return 0;
    let acc = 0;
    for (let i = 0; i < samples.length; i++) acc += samples[i] * samples[i];
    return Math.sqrt(acc / samples.length);
}
