export function isLoopback(host: string): boolean {
    const bare = host
        .trim()
        .toLowerCase()
        .replace(/^\[(.*)\]$/, '$1');
    return bare === 'localhost' || bare === '::1' || /^127\.\d+\.\d+\.\d+$/.test(bare);
}
