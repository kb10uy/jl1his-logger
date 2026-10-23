export function uuidv7(now: number = Date.now(), random: Uint8Array = randomBytes()): string {
    const bytes = new Uint8Array(16);
    let time = BigInt(now);
    for (let i = 5; i >= 0; i--) {
        bytes[i] = Number(time & 0xffn);
        time >>= 8n;
    }
    bytes.set(random.subarray(0, 10), 6);
    bytes[6] = 0x70 | (bytes[6] & 0x0f);
    bytes[8] = 0x80 | (bytes[8] & 0x3f);
    const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
    return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

function randomBytes(): Uint8Array {
    return crypto.getRandomValues(new Uint8Array(10));
}
