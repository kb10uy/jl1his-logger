import type { Fields, Station } from '#lib/api/types.ts';

export function isoDate(adif: string | undefined): string {
    const d = (adif ?? '').trim();
    return /^\d{8}$/.test(d) ? `${d.slice(0, 4)}-${d.slice(4, 6)}-${d.slice(6, 8)}` : d;
}

export function hhmm(adif: string | undefined): string {
    const t = (adif ?? '').trim();
    return /^\d{4}(\d{2})?$/.test(t) ? `${t.slice(0, 2)}:${t.slice(2, 4)}` : t;
}

export function hhmmss(adif: string | undefined): string {
    const t = (adif ?? '').trim();
    if (/^\d{6}$/.test(t)) return `${t.slice(0, 2)}:${t.slice(2, 4)}:${t.slice(4, 6)}`;
    return hhmm(t);
}

export function adifDate(iso: string): string {
    return iso.replaceAll('-', '');
}

export function adifTime(clock: string): string {
    const digits = clock.replaceAll(':', '');
    return digits.length === 4 ? `${digits}00` : digits;
}

export function utcFields(at: Date): { QSO_DATE: string; TIME_ON: string } {
    const iso = at.toISOString();
    return {
        QSO_DATE: iso.slice(0, 10).replaceAll('-', ''),
        TIME_ON: iso.slice(11, 19).replaceAll(':', '')
    };
}

export function utcClock(at: Date): { date: string; time: string } {
    const iso = at.toISOString();
    return { date: iso.slice(0, 10), time: iso.slice(11, 19) };
}

export function rst(fields: Fields): string {
    const sent = fields.RST_SENT ?? '';
    const rcvd = fields.RST_RCVD ?? '';
    return sent || rcvd ? `${sent || '—'} / ${rcvd || '—'}` : '';
}

export function parks(fields: Fields): string[] {
    return (fields.MY_POTA_REF ?? '')
        .split(',')
        .map((p) => p.trim())
        .filter((p) => p !== '');
}

export function stationLabel(station: Station): string {
    const park = parks(station.fields)[0];
    return [station.callsign, station.name, park].filter((s) => s).join(' · ');
}

export function qsoTime(fields: Fields): number {
    const date = (fields.QSO_DATE ?? '').trim();
    const time = adifTime((fields.TIME_ON ?? '').trim()).padEnd(6, '0');
    const parsed = Date.UTC(
        Number(date.slice(0, 4)),
        Number(date.slice(4, 6)) - 1,
        Number(date.slice(6, 8)),
        Number(time.slice(0, 2)),
        Number(time.slice(2, 4)),
        Number(time.slice(4, 6))
    );
    return Number.isNaN(parsed) ? 0 : parsed / 1000;
}

export function compareNewestFirst(
    a: { id: string; fields: Fields },
    b: { id: string; fields: Fields }
): number {
    return qsoTime(b.fields) - qsoTime(a.fields) || (a.id < b.id ? 1 : a.id > b.id ? -1 : 0);
}
