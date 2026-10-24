import { describe, expect, it } from 'vitest';
import { areaDraftOf, areaInput, areaProblem } from './area.ts';

describe('area field drafts', () => {
    it('round-trips the field and reads an empty draft as none', () => {
        const jcx = { name: 'CNTY', label: 'JCC / JCG' };
        expect(areaInput(areaDraftOf(jcx))).toEqual(jcx);
        expect(areaInput(areaDraftOf(null))).toBeNull();
        expect(areaInput({ name: ' cnty ', label: ' County ' })).toEqual({
            name: 'CNTY',
            label: 'County'
        });
    });

    it.each([
        [{ name: '', label: '' }, undefined],
        [{ name: 'STATE', label: 'State' }, undefined],
        [{ name: 'APP_X', label: 'X' }, undefined],
        [{ name: '', label: 'County' }, 'Name the ADIF field, or clear the label.'],
        [{ name: 'CNTY', label: ' ' }, 'Give the field a label.'],
        [{ name: 'JCC', label: 'JCC' }, 'JCC is not an ADIF QSO field.'],
        [{ name: 'my_cnty', label: 'County' }, 'MY_CNTY has its own place on the screens.'],
        [{ name: 'NAME', label: 'Name' }, 'NAME has its own place on the screens.']
    ])('judges %o', (draft, problem) => {
        expect(areaProblem(draft)).toBe(problem);
    });
});
