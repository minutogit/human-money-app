/**
 * Utilities for building and parsing Human Money custom desktop URIs and text templates.
 * 
 * Supports:
 * - Contact cards: humoco://contact?did=...&name=...
 * - Payment requests: humoco://pay?to=...&amount=...&unit=...&std=...&note=...
 * - Market contact cards: humoco://card?env=...
 */

import type { TFunction } from 'i18next';

export interface ContactCardParams {
  did: string;
  name?: string;
  syncUrls?: string[];
}

export interface ParsedContactCard {
  did: string;
  name?: string;
  syncUrls?: string[];
}

export interface PaymentRequestParams {
  to: string;
  name?: string;
  standardUuid?: string;
  standardName?: string;
  unit?: string;
  amount?: string | number;
  note?: string;
  isTest?: boolean;
}

export interface ParsedPaymentRequest {
  to: string;
  name?: string;
  standardUuid?: string;
  standardName?: string;
  unit?: string;
  amount?: string;
  note?: string;
  isTest?: boolean;
}

const CONTACT_URI_REGEX = /humoco:\/\/contact\?([^\s<>"'`]+)/i;
const PAY_URI_REGEX = /humoco:\/\/pay\?([^\s<>"'`]+)/i;
const CARD_URI_REGEX = /humoco:\/\/card\?([^\s<>"'`]+)/i;

const TRAILING_PUNCT_REGEX = /[.,;:!?)\]"'>]+$/;
const CONTINUATION_LINE_REGEX = /^[A-Za-z0-9\-_=+&%.:/]+$/;

function stripTrailingPunctuation(query: string): string {
  return query.replace(TRAILING_PUNCT_REGEX, '');
}

function unwrapHumocoUriLines(input: string, schemePrefix: string): string {
  const lower = input.toLowerCase();
  const idx = lower.indexOf(schemePrefix.toLowerCase());
  if (idx === -1) return input;
  const slice = input.slice(idx);
  const lines = slice.split(/\r?\n/);
  let joined = (lines[0] || '').trim();
  for (let i = 1; i < lines.length; i++) {
    const line = lines[i].trim();
    if (!line) break;
    const stripped = line.replace(TRAILING_PUNCT_REGEX, '');
    if (stripped.length > 0 && CONTINUATION_LINE_REGEX.test(stripped)) {
      joined += line;
    } else {
      break;
    }
  }
  return joined;
}

export interface ParsedCardUri {
  envBase64: string;
  envelopeJson?: string;
}

function compactSyncUrlForUri(raw: string): string {
  const trimmed = raw.trim();
  const withoutHttps = trimmed.replace(/^https:\/\//i, '');
  return withoutHttps.replace(/\/+$/, '').replace(/\/sync\.php$/i, '').replace(/\/+$/, '');
}

/**
 * Builds a canonical humoco://contact URI with optional name and up to 2 compact sync URLs.
 */
export function buildContactUri({ did, name, syncUrls }: ContactCardParams): string {
  const cleanDid = did.trim();
  const cleanName = name?.trim();

  const params = new URLSearchParams();
  params.set('did', cleanDid);
  if (cleanName) {
    params.set('name', cleanName);
  }
  if (syncUrls && syncUrls.length > 0) {
    const valid = syncUrls
      .map(s => s.trim())
      .filter(s => !/^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//.test(s) || /^https?:\/\//i.test(s))
      .map(compactSyncUrlForUri)
      .filter(Boolean)
      .slice(0, 2);
    for (const url of valid) {
      params.append('sync', url);
    }
  }

  return `humoco://contact?${params.toString().replace(/\+/g, '%20').replace(/\(/g, '%28').replace(/\)/g, '%29')}`;
}

/**
 * Parses a contact card from raw text or humoco://contact URI.
 * 
 * Handles:
 * 1. Embedded humoco://contact?did=...&name=...&sync=... inside multiline emails/chat messages
 * 2. Standalone DID string (did:key:... or prefix:tag@did:key:...)
 */
export function parseContactCard(input: string): ParsedContactCard | null {
  if (!input || typeof input !== 'string') return null;

  const unwrapped = unwrapHumocoUriLines(input, 'humoco://contact?');
  const match = unwrapped.match(CONTACT_URI_REGEX);
  if (match && match[1]) {
    try {
      const searchParams = new URLSearchParams(stripTrailingPunctuation(match[1]));
      const did = searchParams.get('did')?.trim();
      const name = searchParams.get('name')?.trim() || undefined;
      const rawSync = searchParams.getAll('sync');
      const syncUrls = rawSync
        .map(s => s.trim())
        .filter(s => s.length > 0 && (!/^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//.test(s) || /^https?:\/\//i.test(s)))
        .map(s => normalizeSyncUrlInput(s))
        .filter(Boolean)
        .slice(0, 2);

      if (did && (did.startsWith('did:') || did.includes('@did:'))) {
        return {
          did,
          name,
          syncUrls: syncUrls.length > 0 ? syncUrls : undefined,
        };
      }
    } catch {
      // Fall through if URL parsing fails
    }
  }

  const trimmed = input.trim();
  // Check if the trimmed input is a standalone DID
  if ((trimmed.startsWith('did:') || trimmed.includes('@did:')) && !/\s/.test(trimmed)) {
    return { did: trimmed };
  }

  // Fallback: extract embedded did:key:... (optionally with prefix:tag@) from chat/email text
  const embeddedDidMatch = trimmed.match(
    /(?:[a-zA-Z0-9_-]+:[a-zA-Z0-9_-]+@)?did:key:[1-9A-HJ-NP-Za-km-z]+/
  );
  if (embeddedDidMatch) {
    return { did: embeddedDidMatch[0] };
  }

  return null;
}

/**
 * Builds the compact humoco://pay URI for QR codes and links.
 */
export function buildPaymentRequestUri(params: PaymentRequestParams): string {
  const cleanTo = (params.to || '').trim();
  const cleanName = params.name?.trim();
  const cleanUnit = params.unit?.trim();
  const cleanStdUuid = params.standardUuid?.trim();
  const cleanStdName = params.standardName?.trim();
  const cleanNote = params.note?.trim();
  const amountStr = params.amount !== undefined && params.amount !== null && String(params.amount).trim() !== ''
    ? String(params.amount).trim()
    : undefined;

  const searchParams = new URLSearchParams();
  if (cleanTo) searchParams.set('to', cleanTo);
  if (cleanName) searchParams.set('name', cleanName);
  if (amountStr) searchParams.set('amount', amountStr);
  if (cleanUnit) searchParams.set('unit', cleanUnit);
  if (cleanStdUuid) searchParams.set('std', cleanStdUuid);
  if (cleanStdName) searchParams.set('stdName', cleanStdName);
  if (cleanNote) searchParams.set('note', cleanNote);
  if (params.isTest) searchParams.set('test', 'true');

  return `humoco://pay?${searchParams.toString().replace(/\+/g, '%20').replace(/\(/g, '%28').replace(/\)/g, '%29')}`;
}

export function buildPaymentRequestText(params: PaymentRequestParams, t?: TFunction): string {
  const tr = (key: string, fallback: string) => (t ? t(key, { defaultValue: fallback }) : fallback);
  const cleanTo = (params.to || '').trim();
  const cleanName = params.name?.trim();
  const cleanUnit = params.unit?.trim();
  const cleanNote = params.note?.trim();
  const amountStr = params.amount !== undefined && params.amount !== null && String(params.amount).trim() !== ''
    ? String(params.amount).trim()
    : undefined;

  const uri = buildPaymentRequestUri(params);

  const heading = tr('transfer.paymentRequestText.heading', 'Human Money Zahlungsanforderung:');
  const recipientLabel = tr('transfer.paymentRequestText.recipient', 'Empfänger');
  const amountLabel = tr('transfer.paymentRequestText.amount', 'Betrag');
  const currencyLabel = tr('transfer.paymentRequestText.currency', 'Währung');
  const noteLabel = tr('transfer.paymentRequestText.note', 'Verwendungszweck');

  const lines: string[] = [heading];
  if (cleanName && cleanTo) {
    lines.push(`${recipientLabel}: ${cleanName} (${cleanTo})`);
  } else if (cleanName) {
    lines.push(`${recipientLabel}: ${cleanName}`);
  } else if (cleanTo) {
    lines.push(`${recipientLabel}: ${cleanTo}`);
  }

  if (amountStr && cleanUnit) {
    lines.push(`${amountLabel}: ${amountStr} ${cleanUnit}`);
  } else if (amountStr) {
    lines.push(`${amountLabel}: ${amountStr}`);
  } else if (cleanUnit) {
    lines.push(`${currencyLabel}: ${cleanUnit}`);
  }

  if (cleanNote) {
    lines.push(`${noteLabel}: ${cleanNote}`);
  }

  lines.push('');
  lines.push(uri);

  return lines.join('\n');
}

/**
 * Parses a payment request from arbitrary chat/email text.
 * 
 * Extracts humoco://pay?... and ensures 'to' starts with 'did:'.
 */
export function parsePaymentRequest(input: string): ParsedPaymentRequest | null {
  if (!input || typeof input !== 'string') return null;

  const unwrapped = unwrapHumocoUriLines(input, 'humoco://pay?');
  const match = unwrapped.match(PAY_URI_REGEX);
  if (!match || !match[1]) return null;

  try {
    const searchParams = new URLSearchParams(stripTrailingPunctuation(match[1]));
    const to = searchParams.get('to')?.trim();

    if (!to || (!to.startsWith('did:') && !to.includes('@did:'))) {
      return null;
    }

    const name = searchParams.get('name')?.trim() || undefined;
    const standardUuid = searchParams.get('std')?.trim() || undefined;
    const standardName = searchParams.get('stdName')?.trim() || undefined;
    const unit = searchParams.get('unit')?.trim() || undefined;
    const rawAmount = searchParams.get('amount')?.trim();
    const amount = rawAmount ? rawAmount.replace(',', '.') : undefined;
    const note = searchParams.get('note')?.trim() || undefined;

    const testParam = searchParams.get('test');
    const isTest = testParam !== null ? (testParam === 'true' || testParam === '1') : undefined;

    return {
      to,
      name,
      standardUuid,
      standardName,
      unit,
      amount,
      note,
      isTest,
    };
  } catch {
    return null;
  }
}

/**
 * Parses a humoco://card URI from arbitrary text.
 * 
 * Extracts humoco://card?env=... and decodes the CardEnvelope JSON if possible.
 */
export function parseCardUri(input: string): ParsedCardUri | null {
  if (!input || typeof input !== 'string') return null;

  const unwrapped = unwrapHumocoUriLines(input, 'humoco://card?');
  const match = unwrapped.match(CARD_URI_REGEX);
  if (!match || !match[1]) return null;

  try {
    const searchParams = new URLSearchParams(stripTrailingPunctuation(match[1]));
    const env = searchParams.get('env')?.trim();
    if (!env) return null;

    let envelopeJson: string | undefined;
    try {
      const b64 = env.replace(/-/g, '+').replace(/_/g, '/');
      const binStr = typeof window !== 'undefined' ? window.atob(b64) : '';
      if (binStr) {
        const bytes = new Uint8Array(binStr.length);
        for (let i = 0; i < binStr.length; i++) {
          bytes[i] = binStr.charCodeAt(i);
        }
        envelopeJson = new TextDecoder().decode(bytes);
      }
    } catch {
      // Decode fallback
    }

    return {
      envBase64: env,
      envelopeJson,
    };
  } catch {
    return null;
  }
}

/**
 * Normalizes user-entered or pasted Sync Service URLs (extracts embedded http/https URLs from chat text,
 * enforces https except loopback, strips query/fragment, appends `/sync.php` cleanly).
 */
export function normalizeSyncUrlInput(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return '';

  const embeddedUrl = trimmed.match(/https?:\/\/[^\s<>"'`]+/i);
  let s = embeddedUrl ? embeddedUrl[0].replace(/[.,;:!?)\]]+$/, '') : trimmed;

  if (!s.includes('://')) {
    s = 'https://' + s;
  } else if (s.toLowerCase().startsWith('http://')) {
    const after = s.slice(7);
    const host = after.split(/[/:]/)[0].toLowerCase();
    if (host !== 'localhost' && host !== '127.0.0.1' && host !== '[::1]') {
      s = 'https://' + after;
    }
  }

  s = s.replace(/[#?].*$/, '');

  const schemeIdx = s.indexOf('://');
  const scheme = s.substring(0, schemeIdx + 3).toLowerCase();
  const rest = s.substring(schemeIdx + 3);

  const firstSlash = rest.indexOf('/');
  let authority = firstSlash === -1 ? rest : rest.substring(0, firstSlash);
  const rawPath = firstSlash === -1 ? '/' : rest.substring(firstSlash);

  authority = authority.toLowerCase();

  if (scheme === 'https://' && authority.endsWith(':443')) {
    authority = authority.slice(0, -4);
  } else if (scheme === 'http://' && authority.endsWith(':80')) {
    authority = authority.slice(0, -3);
  }

  const cleanPath = rawPath.replace(/\/+$/, '');
  let canonicalPath: string;
  if (!cleanPath || cleanPath === '') {
    canonicalPath = '/sync.php';
  } else if (cleanPath.endsWith('/sync.php') || cleanPath === '/sync.php') {
    canonicalPath = cleanPath;
  } else {
    canonicalPath = `${cleanPath}/sync.php`;
  }

  return `${scheme}${authority}${canonicalPath}`;
}

