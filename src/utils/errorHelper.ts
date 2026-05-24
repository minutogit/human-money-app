import { TFunction } from 'i18next';
import { BackendError } from '../types';

/**
 * Type guard: checks if the caught error is a structured BackendError DTO.
 */
function isBackendError(error: unknown): error is BackendError {
  return (
    typeof error === 'object' &&
    error !== null &&
    'code' in error &&
    'message' in error &&
    typeof (error as BackendError).code === 'string'
  );
}

/**
 * Translates a backend error into a localized user-facing message.
 *
 * - Structured BackendError: uses t(code, details) with English fallback.
 * - Plain string: returns directly (legacy path).
 * - Unknown: generic fallback.
 */
export function translateError(error: unknown, t: TFunction): string {
  if (isBackendError(error)) {
    return t(error.code, {
      ...error.details,
      defaultValue: error.message,
    });
  }
  if (typeof error === 'string') {
    return error;
  }
  return t('error.internal.unknown', { defaultValue: 'An unexpected error occurred.' });
}
