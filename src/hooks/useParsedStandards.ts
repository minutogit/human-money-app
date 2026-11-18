// src/hooks/useParsedStandards.ts
import { useMemo, useCallback } from 'react';
import { VoucherStandardInfo } from '../types';

export interface ParsedStandardRecord {
    id: string;
    displayName: string;
    content: string;
    uuid: string | null;
    name: string | null;
    abbreviation: string | null;
    unit: string | null;
    decimalPlaces: number;
    allowPartialTransfers: boolean | null;
    balancesAreSummable: boolean | null;
    privacyMode: string | null;
    issuerName: string | null;
    defaultValidityDuration: string | null;
    raw: VoucherStandardInfo;
}

const DEFAULT_DECIMAL_PLACES = 4;

function stripTomlComment(line: string): string {
    let inSingle = false;
    let inDouble = false;
    let escaped = false;

    for (let i = 0; i < line.length; i += 1) {
        const ch = line[i];
        if (escaped) {
            escaped = false;
            continue;
        }
        if (inDouble) {
            if (ch === '\\') {
                escaped = true;
            } else if (ch === '"') {
                inDouble = false;
            }
            continue;
        }
        if (inSingle) {
            if (ch === "'") {
                inSingle = false;
            }
            continue;
        }
        if (ch === '"') {
            inDouble = true;
        } else if (ch === "'") {
            inSingle = true;
        } else if (ch === '#') {
            return line.slice(0, i);
        }
    }
    return line;
}

function parseTomlScalar(rawValue: string): string | number | boolean | null {
    const trimmed = rawValue.trim();
    if (!trimmed) return null;

    if (trimmed.startsWith('"') && trimmed.endsWith('"') && trimmed.length >= 2) {
        return trimmed
            .slice(1, -1)
            .replace(/\\"/g, '"')
            .replace(/\\\\/g, '\\');
    }
    if (trimmed.startsWith("'") && trimmed.endsWith("'") && trimmed.length >= 2) {
        return trimmed.slice(1, -1);
    }
    if (trimmed === 'true') return true;
    if (trimmed === 'false') return false;
    if (/^-?\d+$/.test(trimmed)) {
        const num = Number.parseInt(trimmed, 10);
        return Number.isNaN(num) ? null : num;
    }
    return null;
}

/**
 * Synchronously extracts key metadata fields from a voucher standard's raw TOML content.
 * Supports both full sectioned standard TOML and flat test fixture TOML snippets.
 */
export function parseStandardInfo(standard: VoucherStandardInfo): ParsedStandardRecord {
    const content = typeof standard.content === 'string' ? standard.content : '';
    const record: ParsedStandardRecord = {
        id: standard.id,
        displayName: standard.displayName,
        content,
        uuid: null,
        name: null,
        abbreviation: null,
        unit: null,
        decimalPlaces: DEFAULT_DECIMAL_PLACES,
        allowPartialTransfers: null,
        balancesAreSummable: null,
        privacyMode: null,
        issuerName: null,
        defaultValidityDuration: null,
        raw: standard,
    };

    let section = '';
    const lines = content.split(/\r?\n/);

    for (const rawLine of lines) {
        const line = stripTomlComment(rawLine).trim();
        if (!line) continue;

        if (line.startsWith('[') && line.endsWith(']')) {
            section = line.slice(1, -1).trim();
            continue;
        }

        const kvMatch = line.match(/^([A-Za-z0-9_]+)\s*=\s*(.+)$/);
        if (!kvMatch) continue;

        const key = kvMatch[1];
        const value = parseTomlScalar(kvMatch[2]);
        if (value === null) continue;

        const inTopLevel = section === '';

        if ((inTopLevel || section === 'immutable.identity') && typeof value === 'string') {
            if (key === 'uuid') record.uuid = value;
            else if (key === 'name') record.name = value;
            else if (key === 'abbreviation') record.abbreviation = value;
        } else if ((inTopLevel || section === 'immutable.blueprint') && typeof value === 'string') {
            if (key === 'unit') record.unit = value;
        } else if (inTopLevel || section === 'immutable.features') {
            if ((key === 'amount_decimal_places' || key === 'amountDecimalPlaces') && typeof value === 'number') {
                record.decimalPlaces = value;
            } else if ((key === 'allow_partial_transfers' || key === 'allowPartialTransfers') && typeof value === 'boolean') {
                record.allowPartialTransfers = value;
            } else if ((key === 'balances_are_summable' || key === 'balancesAreSummable') && typeof value === 'boolean') {
                record.balancesAreSummable = value;
            } else if ((key === 'privacy_mode' || key === 'privacyMode') && typeof value === 'string') {
                record.privacyMode = value;
            }
        } else if ((inTopLevel || section === 'mutable.metadata') && typeof value === 'string') {
            if (key === 'issuer_name' || key === 'issuerName') {
                record.issuerName = value;
            }
        } else if (
            (inTopLevel || section === 'mutable.app_config' || section === 'mutable.appConfig') &&
            typeof value === 'string'
        ) {
            if (key === 'default_validity_duration' || key === 'defaultValidityDuration') {
                record.defaultValidityDuration = value;
            }
        }
    }

    return record;
}

/**
 * Parses raw TOML VoucherStandardInfo records once via useMemo and provides
 * O(1) lookup helpers by standard UUID or ID.
 */
export function useParsedStandards(standards: readonly VoucherStandardInfo[] = []) {
    const { parsedMap, byUuid } = useMemo(() => {
        const byId = new Map<string, ParsedStandardRecord>();
        const uuidIndex = new Map<string, ParsedStandardRecord>();

        for (const std of standards) {
            if (!std) continue;
            const parsed = parseStandardInfo(std);
            byId.set(std.id, parsed);
            if (parsed.uuid && !uuidIndex.has(parsed.uuid)) {
                uuidIndex.set(parsed.uuid, parsed);
            }
        }

        return { parsedMap: byId, byUuid: uuidIndex };
    }, [standards]);

    const findStandardByUuid = useCallback(
        (uuidOrId?: string | null): ParsedStandardRecord | undefined => {
            if (!uuidOrId) return undefined;
            const key = uuidOrId.trim();
            if (!key) return undefined;
            return byUuid.get(key) ?? parsedMap.get(key);
        },
        [byUuid, parsedMap]
    );

    const getStandardName = useCallback(
        (uuidOrId?: string | null, fallback?: string): string => {
            if (!uuidOrId) return fallback ?? '';
            const found = findStandardByUuid(uuidOrId);
            return found?.name || found?.displayName || fallback || uuidOrId;
        },
        [findStandardByUuid]
    );

    return {
        parsedMap,
        findStandardByUuid,
        getStandardName,
        standards,
    };
}
