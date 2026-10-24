export function isPower(value: string): boolean {
    return /^(\d+(\.\d{0,6})?|\.\d{1,6})$/.test(value) && Number(value) > 0;
}

export const POWER_RULE = 'TX power is watts above zero, with at most six decimal places.';
