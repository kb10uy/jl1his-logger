import {
    ADIF_BANDS,
    ADIF_DXCC_ENTITIES,
    ADIF_FIELD_ENUMERATIONS,
    ADIF_MODES
} from '#lib/adif/tables.ts';
import { m } from '#lib/paraglide/messages.js';
import { QSL_RCVD_VALUES, QSL_SENT_VALUES, QSL_VIA_VALUES, type QslChoice } from './qsl.ts';

export type FieldChoice = [value: string, label: string];

const labeled = (choices: QslChoice[]): FieldChoice[] =>
    choices.map(([code, label]) => [code, label()]);

export const enumerationOf = (name: string): string | undefined =>
    ADIF_FIELD_ENUMERATIONS[name.trim().toUpperCase()];

export function fieldChoices(name: string): FieldChoice[] | undefined {
    switch (enumerationOf(name)) {
        case 'Band':
            return ADIF_BANDS.map((b) => [b.name, '']);
        case 'Mode':
            return ADIF_MODES.map((mode) => [mode.mode, '']);
        case 'QSL_Sent':
            return labeled(QSL_SENT_VALUES);
        case 'QSL_Rcvd':
            return labeled(QSL_RCVD_VALUES);
        case 'QSL_Via':
            return labeled(QSL_VIA_VALUES);
        case 'DXCC_Entity_Code':
            return [...ADIF_DXCC_ENTITIES]
                .sort((a, b) => Number(a.deleted) - Number(b.deleted) || a.code - b.code)
                .map((e) => [
                    String(e.code),
                    e.deleted ? `${e.name} (${m.stations_dxcc_deleted()})` : e.name
                ]);
        default:
            return undefined;
    }
}
