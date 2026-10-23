export interface Band {
    name: string;
    lowerHz: number;
    upperHz: number;
    representativeHz: number;
    label: string;
}

const band = (
    name: string,
    lowerHz: number,
    upperHz: number,
    representativeHz: number,
    label: string
): Band => ({
    name,
    lowerHz,
    upperHz,
    representativeHz,
    label
});

export const BAND_GROUPS: { label: string; bands: Band[] }[] = [
    {
        label: 'LF / MF',
        bands: [
            band('2190m', 135_700, 137_800, 136_000, '136 kHz'),
            band('630m', 472_000, 479_000, 472_000, '472 kHz'),
            band('560m', 501_000, 504_000, 501_000, '501 kHz'),
            band('160m', 1_800_000, 2_000_000, 1_900_000, '1.8 MHz')
        ]
    },
    {
        label: 'HF',
        bands: [
            band('80m', 3_500_000, 4_000_000, 3_500_000, '3.5 MHz'),
            band('60m', 5_060_000, 5_450_000, 5_300_000, '5.3 MHz'),
            band('40m', 7_000_000, 7_300_000, 7_000_000, '7 MHz'),
            band('30m', 10_100_000, 10_150_000, 10_100_000, '10.1 MHz'),
            band('20m', 14_000_000, 14_350_000, 14_000_000, '14 MHz'),
            band('17m', 18_068_000, 18_168_000, 18_100_000, '18 MHz'),
            band('15m', 21_000_000, 21_450_000, 21_000_000, '21 MHz'),
            band('12m', 24_890_000, 24_990_000, 24_900_000, '24.9 MHz'),
            band('10m', 28_000_000, 29_700_000, 28_000_000, '28 MHz')
        ]
    },
    {
        label: 'VHF',
        bands: [
            band('8m', 40_000_000, 45_000_000, 40_000_000, '40 MHz'),
            band('6m', 50_000_000, 54_000_000, 50_000_000, '50 MHz'),
            band('5m', 54_000_001, 69_900_000, 60_000_000, '54 MHz'),
            band('4m', 70_000_000, 71_000_000, 70_000_000, '70 MHz'),
            band('2m', 144_000_000, 148_000_000, 144_000_000, '144 MHz'),
            band('1.25m', 222_000_000, 225_000_000, 222_000_000, '222 MHz')
        ]
    },
    {
        label: 'UHF',
        bands: [
            band('70cm', 420_000_000, 450_000_000, 430_000_000, '430 MHz'),
            band('33cm', 902_000_000, 928_000_000, 902_000_000, '902 MHz'),
            band('23cm', 1_240_000_000, 1_300_000_000, 1_260_000_000, '1.2 GHz')
        ]
    },
    {
        label: 'SHF and up',
        bands: [
            band('13cm', 2_300_000_000, 2_450_000_000, 2_400_000_000, '2.4 GHz'),
            band('9cm', 3_300_000_000, 3_500_000_000, 3_400_000_000, '3.4 GHz'),
            band('6cm', 5_650_000_000, 5_925_000_000, 5_700_000_000, '5.7 GHz'),
            band('3cm', 10_000_000_000, 10_500_000_000, 10_000_000_000, '10 GHz'),
            band('1.25cm', 24_000_000_000, 24_250_000_000, 24_000_000_000, '24 GHz'),
            band('6mm', 47_000_000_000, 47_200_000_000, 47_000_000_000, '47 GHz'),
            band('4mm', 75_500_000_000, 81_000_000_000, 76_000_000_000, '76 GHz'),
            band('2.5mm', 119_980_000_000, 123_000_000_000, 122_000_000_000, '122 GHz'),
            band('2mm', 134_000_000_000, 149_000_000_000, 134_000_000_000, '134 GHz'),
            band('1mm', 241_000_000_000, 250_000_000_000, 241_000_000_000, '241 GHz'),
            band('submm', 300_000_000_000, 7_500_000_000_000, 300_000_000_000, '300 GHz+')
        ]
    }
];

export const BANDS: Band[] = BAND_GROUPS.flatMap((g) => g.bands);

export function frequencyHz(mhz: string): number | undefined {
    const match = /^(\d*)(?:\.(\d*))?$/.exec(mhz.trim());
    if (match === null) return undefined;
    const [, whole, fraction = ''] = match;
    if (whole === '' && fraction === '') return undefined;
    const micro = Number(fraction.slice(0, 6).padEnd(6, '0'));
    const roundUp = Number(fraction[6] ?? '0') >= 5 ? 1 : 0;
    const hz = Number(whole || '0') * 1_000_000 + micro + roundUp;
    return Number.isSafeInteger(hz) ? hz : undefined;
}

export function formatMhz(hz: number): string {
    return (hz / 1_000_000).toFixed(6).replace(/(\.\d{3}\d*?)0+$/, '$1');
}

export function bandOf(mhz: string): Band | undefined {
    const hz = frequencyHz(mhz);
    if (hz === undefined) return undefined;
    return BANDS.find((b) => hz >= b.lowerHz && hz <= b.upperHz);
}

export function bandNamed(name: string): Band | undefined {
    const wanted = name.trim().toLowerCase();
    return BANDS.find((b) => b.name.toLowerCase() === wanted);
}

export function representativeFreq(name: string): string | undefined {
    const band = bandNamed(name);
    return band && formatMhz(band.representativeHz);
}
