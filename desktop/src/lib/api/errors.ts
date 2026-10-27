import { ApiError } from '#client/api/client.ts';
import { errorText as serverText } from '#client/api/errors.ts';
import { m } from '#lib/paraglide/messages.js';
import { errorText } from '#lib/session/errors.ts';
import type { CommandError } from '#lib/session/types.ts';

export function toApiError(e: CommandError): ApiError {
    switch (e.kind) {
        case 'unreachable':
            return new ApiError(m.common_unreachable(), 0);
        case 'unauthorized':
            return new ApiError(errorText(e), 401);
        case 'server': {
            const body = e.body ?? {};
            return new ApiError(serverText(body) ?? e.detail, e.status, body);
        }
        default:
            return new ApiError(errorText(e), 0);
    }
}
