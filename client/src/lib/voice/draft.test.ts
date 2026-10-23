import { describe, expect, it } from 'vitest';
import { parseSpeech, type SpokenUpdate } from '#lib/voice/dsl/parser.ts';
import {
    anchorHz,
    applyUpdate,
    applyUpdates,
    isPristine,
    newDraft,
    setFreeText,
    withFreq,
    withFrequency,
    withMode,
    type Carry
} from './draft.ts';

const fm: Carry = { freq: '433', band: '70cm', mode: 'FM', submode: '' };
const ssb: Carry = { freq: '', band: '40m', mode: 'SSB', submode: 'LSB' };
const now = new Date('2026-10-03T04:00:00Z');

function spoken(text: string): SpokenUpdate[] {
    const result = parseSpeech(text);
    if (!result.ok) throw new Error(result.error);
    return result.updates;
}

describe('Draft', () => {
    it('starts with the default reports of the carried mode', () => {
        const draft = newDraft(fm);
        expect(draft).toEqual({ ...fm, rstSent: '59', rstRcvd: '59', qsl: 'none' });
        expect(isPristine(draft)).toBe(true);
        expect(newDraft({ ...fm, mode: 'CW' })).toMatchObject({ rstSent: '599', rstRcvd: '599' });
    });

    it('moves only reports still at the default when the mode changes', () => {
        let { draft } = applyUpdates(newDraft(fm), spoken('received five seven'), now);
        ({ draft } = applyUpdates(draft, spoken('mode cw'), now));
        expect([draft.rstSent, draft.rstRcvd]).toEqual(['599', '57']);
        draft = withMode(draft, 'FT8', '');
        expect([draft.rstSent, draft.rstRcvd]).toEqual(['-10', '57']);
    });

    it('updates only the spoken field and records the start time once', () => {
        let draft = applyUpdate(newDraft(fm), { kind: 'rstReceived', value: '57' }, now);
        expect([draft.rstSent, draft.rstRcvd]).toEqual(['59', '57']);
        expect(draft.startedAt).toBe(now.toISOString());
        draft = applyUpdate(
            draft,
            { kind: 'jcx', value: '100101' },
            new Date(now.getTime() + 60_000)
        );
        expect(draft.startedAt).toBe(now.toISOString());
        expect(draft.jcx).toBe('100101');
    });

    it('overwrites a field when it is spoken again', () => {
        let { draft } = applyUpdates(newDraft(fm), spoken('juliett lima one hotel india sierra'));
        ({ draft } = applyUpdates(draft, spoken('juliett lima one hotel india tango')));
        expect(draft.call).toBe('JL1HIT');
    });

    it('records the contest number apart from the RST', () => {
        const { draft, descriptions } = applyUpdates(
            newDraft(fm),
            spoken('received five seven number one zero zero one mike'),
            now
        );
        expect([draft.rstRcvd, draft.received]).toEqual(['57', '1001M']);
        expect(descriptions).toEqual(['RST received → 57', 'Number → 1001M']);
    });

    it('resolves a partly spoken frequency against the current one', () => {
        const { draft, descriptions } = applyUpdates(
            newDraft({ ...fm, freq: '433.1' }),
            spoken('frequency point nine four'),
            now
        );
        expect([draft.freq, draft.band]).toEqual(['432.940', '70cm']);
        expect(descriptions).toEqual(['Frequency *.94 → 432.940 MHz']);
    });

    it('resolves against the middle of the band when no frequency is set', () => {
        expect(anchorHz(ssb)).toBe(7_150_000);
        const { draft } = applyUpdates(newDraft(ssb), spoken('frequency point one four four'));
        expect([draft.freq, draft.band]).toEqual(['7.144', '40m']);
    });

    it('moves the band with the frequency and the sideband with the band', () => {
        const draft = withFrequency(newDraft(ssb), 14_200_000);
        expect(draft).toMatchObject({ freq: '14.200', band: '20m', submode: 'USB' });
        const cw = withFrequency(newDraft({ ...ssb, mode: 'CW', submode: '' }), 14_020_000);
        expect(cw).toMatchObject({ band: '20m', submode: '' });
    });

    it('keeps the band for a frequency outside every band', () => {
        expect(withFrequency(newDraft(ssb), 7_400_000)).toMatchObject({
            freq: '7.400',
            band: '40m'
        });
    });

    it('takes a typed frequency as it is, moving the band', () => {
        expect(withFreq(newDraft(ssb), '14.074')).toMatchObject({
            freq: '14.074',
            band: '20m',
            submode: 'USB'
        });
        expect(withFreq(newDraft(ssb), '')).toMatchObject({ freq: '', band: '40m' });
    });

    it('takes a spoken submode with its ADIF mode', () => {
        const { draft, descriptions } = applyUpdates(newDraft(ssb), spoken('mode ft4'));
        expect([draft.mode, draft.submode]).toEqual(['MFSK', 'FT4']);
        expect(descriptions).toEqual(['Mode → FT4 (MFSK)']);
    });

    it('describes every update of a chained utterance', () => {
        const { draft, descriptions } = applyUpdates(
            newDraft(ssb),
            spoken(
                'juliett lima one hotel india sierra sent five five card requested mode foxtrot mike'
            ),
            now
        );
        expect(draft).toMatchObject({ call: 'JL1HIS', rstSent: '55', qsl: 'requested' });
        expect([draft.mode, draft.submode]).toEqual(['FM', '']);
        expect(descriptions).toEqual([
            'Call → JL1HIS',
            'RST sent → 55',
            'QSL requested',
            'Mode → FM'
        ]);
    });

    it('sets free text, collapsing whitespace and clearing it when blank', () => {
        let draft = setFreeText(newDraft(fm), 'qth', '  東京都\t港区 ', now);
        expect(draft.qth).toBe('東京都 港区');
        expect(draft.startedAt).toBe(now.toISOString());
        draft = setFreeText(draft, 'qth', '   ', now);
        expect(draft.qth).toBeUndefined();
        expect(isPristine(setFreeText(newDraft(fm), 'name', ' ', now))).toBe(true);
    });

    it('does not mutate the input draft', () => {
        const draft = newDraft(fm);
        applyUpdate(draft, { kind: 'callsign', value: 'JL1HIS' }, now);
        expect(draft.call).toBeUndefined();
    });
});
