export function download(name: string, data: string | Uint8Array<ArrayBuffer>) {
    const type = typeof data === 'string' ? 'text/plain;charset=utf-8' : 'text/plain';
    const url = URL.createObjectURL(new Blob([data], { type }));
    const link = document.createElement('a');
    link.href = url;
    link.download = name;
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export function fromBase64(base64: string): Uint8Array<ArrayBuffer> {
    return Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
}
