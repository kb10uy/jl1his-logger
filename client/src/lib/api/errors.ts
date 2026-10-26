import { m } from '#lib/paraglide/messages.js';

const NOT_FOUND: Readonly<Record<string, () => string>> = {
    QSO: m.error_not_found_qso,
    Station: m.error_not_found_station,
    Instrument: m.error_not_found_instrument,
    Logbook: m.error_not_found_logbook,
    user: m.error_not_found_user,
    passkey: m.error_not_found_passkey,
    'API token': m.error_not_found_token,
    'export batch': m.error_not_found_batch,
    Membership: m.error_not_found_membership
};

export function errorText(body: Record<string, unknown>): string | undefined {
    const text = (key: string) => String(body[key] ?? '');
    const field = text('field');
    const value = text('value');
    const term = text('term');
    switch (body.code) {
        case 'not_found':
            return Object.hasOwn(NOT_FOUND, text('what')) ? NOT_FOUND[text('what')]() : undefined;
        case 'name_required':
            return m.error_name_required();
        case 'name_too_long':
            return m.error_name_too_long({ bytes: text('bytes'), max: text('max') });
        case 'batch_empty':
            return m.error_batch_empty();
        case 'batch_applied':
            return m.error_batch_applied();
        case 'batch_nothing_to_apply':
            return m.error_batch_nothing_to_apply({ kind: text('kind') });
        case 'area_field_not_allowed':
            return m.error_area_field_not_allowed({ field });
        case 'last_owner':
            return m.error_last_owner();
        case 'last_passkey':
            return m.error_last_passkey();
        case 'passkey_registered':
            return m.error_passkey_registered();
        case 'field_repeated':
            return m.error_field_repeated({ field });
        case 'field_not_allowed':
            return m.error_field_not_allowed({ field });
        case 'instrument_field':
            return m.error_instrument_field({ field });
        case 'wavelog_id_taken':
            return m.error_wavelog_id_taken({ id: text('id') });
        case 'qso_deleted':
            return m.error_qso_deleted();
        case 'revision_conflict':
            return m.error_revision_conflict();
        case 'qso_changed':
            return m.error_qso_changed();
        case 'id_taken':
            return m.error_id_taken();
        case 'rewrite_count_changed':
            return m.error_rewrite_count_changed({
                count: text('count'),
                expected: text('expected')
            });
        case 'too_many_values':
            return m.error_too_many_values({ count: text('count'), max: text('max') });
        case 'token_needs_logbook':
            return m.error_token_needs_logbook();
        case 'wl2_token_one_logbook':
            return m.error_wl2_token_one_logbook();
        case 'wl2_prefix_only':
            return m.error_wl2_prefix_only();
        case 'not_a_callsign':
            return m.error_not_a_callsign({ callsign: text('callsign') });
        case 'qso_missing':
            return m.error_qso_missing({ field });
        case 'qso_invalid':
            return m.error_qso_invalid({ field, value });
        case 'qso_field_name':
            return m.error_qso_field_name({ field });
        case 'qso_column_field':
            return m.error_qso_column_field({ field });
        case 'qso_too_long':
            return m.error_qso_too_long({ field, bytes: text('bytes'), max: text('max') });
        case 'qso_too_many_fields':
            return m.error_qso_too_many_fields({ count: text('count'), max: text('max') });
        case 'passkey_failed':
            return m.error_passkey_failed();
        case 'passkey_expired':
            return m.error_passkey_expired();
        case 'passkey_unknown':
            return m.error_passkey_unknown();
        case 'registration_link_invalid':
            return m.error_registration_link_invalid();
        case 'not_utf8':
            return m.error_not_utf8();
        case 'not_encoding':
            return m.error_not_encoding({ encoding: text('encoding') });
        case 'unencodable':
            return m.error_unencodable({
                record: text('record'),
                field,
                character: text('character')
            });
        case 'not_adif':
            return m.error_not_adif({ detail: text('detail') });
        case 'unclosed_quote':
            return m.error_query_unclosed_quote({ term });
        case 'empty':
            return m.error_query_empty({ term });
        case 'unknown_field':
            return m.error_query_unknown_field({ value });
        case 'bad_field_name':
            return m.error_query_bad_field_name({ value });
        case 'missing_value':
            return m.error_query_missing_value({ term });
        case 'bad_date':
            return m.error_query_bad_date({ value });
        case 'bad_number':
            return m.error_query_bad_number({ value });
        default:
            return undefined;
    }
}
