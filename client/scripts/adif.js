import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const client = join(dirname(fileURLToPath(import.meta.url)), '..');
const source = join(client, '../assets/adif');
const out = join(client, 'src/lib/adif/tables.ts');
let version;

function parseCsv(text, file) {
    const records = [];
    let record = [];
    let field = '';
    let quoted = false;
    for (let i = 0; i < text.length; i++) {
        const c = text[i];
        if (quoted) {
            if (c === '"' && text[i + 1] === '"') {
                field += '"';
                i++;
            } else if (c === '"') quoted = false;
            else field += c;
        } else if (c === '"' && field === '') quoted = true;
        else if (c === '"')
            throw new Error(`${file} row ${records.length + 1} has a quote inside a value`);
        else if (c === ',') {
            record.push(field);
            field = '';
        } else if (c === '\r' && text[i + 1] === '\n') continue;
        else if (c === '\n') {
            record.push(field);
            records.push(record);
            record = [];
            field = '';
        } else field += c;
    }
    if (quoted)
        throw new Error(`${file} row ${records.length + 1} has an unterminated quoted value`);
    if (field !== '' || record.length > 0) records.push([...record, field]);
    return records;
}

function read(name) {
    const file = `assets/adif/${name}.csv`;
    const text = readFileSync(join(source, `${name}.csv`), 'utf8').replace(/^﻿/, '');
    const [header, ...records] = parseCsv(text, file);
    if (header === undefined) throw new Error(`${file} is empty`);
    if (records.length === 0) throw new Error(`${file} has no rows`);
    return records.map((record, i) => {
        const number = i + 2;
        const fail = (message) => {
            throw new Error(`${file} row ${number}: ${message}`);
        };
        if (record.length !== header.length) {
            fail(`it has ${record.length} values, not the ${header.length} of the header`);
        }
        const cells = new Map(header.map((title, j) => [title, record[j].trim()]));
        const get = (column) => cells.get(column) ?? fail(`there is no column ${column}`);
        const row = {
            fail,
            get,
            required: (column) => get(column) || fail(`${column} is empty`),
            flag: (column) => {
                const value = get(column);
                if (value !== '' && value !== column) fail(`${column} is ${JSON.stringify(value)}`);
                return value === column;
            }
        };
        const rowVersion = row.required('ADIF Version');
        if (version === undefined) version = rowVersion;
        else if (rowVersion !== version) fail(`ADIF Version ${rowVersion} differs from ${version}`);
        return row;
    });
}

function hz(row, column) {
    const mhz = row.required(column);
    const match = /^(\d*)(?:\.(\d{0,6}))?$/.exec(mhz);
    if (match === null || mhz === '.') row.fail(`${column} ${mhz} is not a frequency in MHz`);
    const [, whole, fraction = ''] = match;
    return Number(whole || '0') * 1_000_000 + Number(fraction.padEnd(6, '0'));
}

function bands() {
    const bands = [];
    for (const row of read('enumerations_band')) {
        if (row.flag('Import-only')) continue;
        const band = {
            name: row.required('Band'),
            lowerHz: hz(row, 'Lower Freq (MHz)'),
            upperHz: hz(row, 'Upper Freq (MHz)')
        };
        if (band.lowerHz > band.upperHz) row.fail(`${band.name} ends below its start`);
        const previous = bands.at(-1);
        if (previous && previous.upperHz >= band.lowerHz) {
            row.fail(`${band.name} overlaps ${previous.name} or comes before it`);
        }
        bands.push(band);
    }
    return bands;
}

function modes() {
    const modeRows = read('enumerations_mode');
    const modes = new Map();
    const importOnly = [];
    for (const row of modeRows) {
        const name = row.required('Mode');
        if (modes.has(name) || importOnly.includes(name)) row.fail(`${name} is listed twice`);
        if (row.flag('Import-only')) importOnly.push(name);
        else modes.set(name, []);
    }
    const known = new Map();
    const owners = new Map();
    for (const row of read('enumerations_submode')) {
        if (row.flag('Import-only')) continue;
        const [submode, mode] = [row.required('Submode'), row.required('Mode')];
        if (!modes.has(mode))
            row.fail(`${submode} belongs to ${mode}, which is not a current mode`);
        known.set(mode, [...(known.get(mode) ?? []), submode]);
        owners.set(submode, mode);
    }
    for (const row of modeRows) {
        const name = row.get('Mode');
        const list = modes.get(name);
        if (list === undefined) continue;
        const ofMode = known.get(name) ?? [];
        const listed = row
            .get('Submodes')
            .split(',')
            .map((s) => s.trim())
            .filter((s) => s !== '');
        for (const entry of listed) {
            const parts = entry.split(/\s+/);
            const missingCommas =
                !ofMode.includes(entry) &&
                parts.length > 1 &&
                parts.every((p) => ofMode.includes(p));
            for (const submode of missingCommas ? parts : [entry]) {
                const owner = owners.get(submode);
                if (owner !== undefined && owner !== name) {
                    row.fail(`${submode} is a submode of ${owner} as well as ${name}`);
                }
                if (!list.includes(submode)) list.push(submode);
            }
        }
        for (const submode of ofMode) if (!list.includes(submode)) list.push(submode);
    }
    const seen = new Set();
    for (const submode of [...modes.values()].flat()) {
        if (seen.has(submode.toUpperCase()))
            throw new Error(`the submode ${submode} belongs to two modes`);
        seen.add(submode.toUpperCase());
    }
    for (const name of importOnly) {
        if (!seen.has(name.toUpperCase())) {
            throw new Error(`the import-only mode ${name} is not a submode of any current mode`);
        }
    }
    return { modes: [...modes].map(([mode, submodes]) => ({ mode, submodes })), importOnly };
}

function fields() {
    const names = [];
    for (const row of read('fields')) {
        if (row.flag('Import-only') || row.get('Header Field') === 'Y') continue;
        const name = row.required('Field Name');
        if (names.includes(name)) row.fail(`${name} is listed twice`);
        names.push(name);
    }
    return names;
}

function dxccEntities() {
    const entities = [];
    for (const row of read('enumerations_dxcc_entity_code')) {
        const code = Number(row.required('Entity Code'));
        if (!Number.isInteger(code)) row.fail(`${row.get('Entity Code')} is not an entity code`);
        if (entities.some((e) => e.code === code)) row.fail(`entity ${code} is listed twice`);
        entities.push({ code, name: row.required('Entity Name'), deleted: row.flag('Deleted') });
    }
    return entities.sort((a, b) => a.code - b.code);
}

const values = (name, column) => read(name).map((row) => row.required(column));

const { modes: modeTable, importOnly } = modes();
const tables = [
    ['ADIF_BANDS', 'readonly AdifBand[]', bands()],
    ['ADIF_MODES', 'readonly AdifMode[]', modeTable],
    ['ADIF_IMPORT_ONLY_MODES', 'readonly string[]', importOnly],
    ['ADIF_QSO_FIELDS', 'readonly string[]', fields()],
    ['ADIF_DXCC_ENTITIES', 'readonly AdifDxccEntity[]', dxccEntities()],
    ['ADIF_QSL_SENT', 'readonly string[]', values('enumerations_qsl_sent', 'Status')],
    ['ADIF_QSL_RCVD', 'readonly string[]', values('enumerations_qsl_rcvd', 'Status')],
    ['ADIF_QSL_VIA', 'readonly string[]', values('enumerations_qsl_via', 'Via')]
];
const text = [
    `export const ADIF_VERSION = ${JSON.stringify(version)};`,
    'export interface AdifBand {\n    name: string;\n    lowerHz: number;\n    upperHz: number;\n}',
    'export interface AdifMode {\n    mode: string;\n    submodes: readonly string[];\n}',
    'export interface AdifDxccEntity {\n    code: number;\n    name: string;\n    deleted: boolean;\n}',
    ...tables.map(
        ([name, type, value]) =>
            `export const ${name}: ${type} = ${JSON.stringify(value, null, 4)};`
    )
].join('\n\n');
mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, `${text}\n`, 'utf8');
console.log(`${out} (ADIF ${version})`);
