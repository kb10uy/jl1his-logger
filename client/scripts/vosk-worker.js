import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const client = join(dirname(fileURLToPath(import.meta.url)), '..');
const bundle = readFileSync(join(client, 'node_modules/vosk-browser/dist/vosk.js'), 'utf8');
const match = /createBase64WorkerFactory\(\s*'([A-Za-z0-9+/=]+)'/.exec(bundle);
if (match === null) throw new Error('vosk-browser has no inline worker to extract');
const source = Buffer.from(match[1], 'base64').toString('latin1');
const body = source.slice(source.indexOf('\n', 10) + 1);
const out = join(client, 'static/vosk-worker.js');
mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, body, 'utf8');
console.log(`${out} (${body.length} characters)`);
