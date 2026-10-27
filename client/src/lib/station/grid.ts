const letter = (base: string, index: number) => String.fromCharCode(base.charCodeAt(0) + index);

export function gridSquare(latitude: number, longitude: number): string | undefined {
    if (!Number.isFinite(latitude) || !Number.isFinite(longitude)) return undefined;
    if (Math.abs(latitude) > 90 || Math.abs(longitude) > 180) return undefined;
    const lon = Math.min(longitude + 180, 360 - 1e-9);
    const lat = Math.min(latitude + 90, 180 - 1e-9);
    return (
        letter('A', Math.floor(lon / 20)) +
        letter('A', Math.floor(lat / 10)) +
        Math.floor((lon % 20) / 2) +
        Math.floor(lat % 10) +
        letter('a', Math.floor((lon % 2) * 12)) +
        letter('a', Math.floor((lat % 1) * 24))
    );
}
