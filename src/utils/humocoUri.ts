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

const CONTACT_URI_REGEX = /humoco:\/\/contact\?([^\s<>"'`„“”‚‘’«»‹›]+)/i;
const PAY_URI_REGEX = /humoco:\/\/pay\?([^\s<>"'`„“”‚‘’«»‹›]+)/i;
const CARD_URI_REGEX = /humoco:\/\/card\?([^\s<>"'`„“”‚‘’«»‹›]+)/i;

const TRAILING_PUNCT_REGEX = /[.,;:!?)\]"'>`„“”‚‘’«»‹›]+$/;
const CONTINUATION_LINE_REGEX = /^[A-Za-z0-9\-_=+&%.:/@]+$/;
const GREETING_WORDS = new Set([
  'danke',
  'hallo',
  'bitte',
  'ciao',
  'ok',
  'lg',
  'mfg',
  'cheers',
  'thanks',
  'regards',
  'gruesse',
  'grüße',
]);

export function isWrappedUriContinuationLine(line: string, prevLineLength: number): boolean {
  if (!line) return false;
  const stripped = line.replace(TRAILING_PUNCT_REGEX, '');
  if (!stripped || !CONTINUATION_LINE_REGEX.test(stripped)) return false;
  if (/[!?]$/.test(line) && /^[A-Za-z.]+$/.test(stripped) && stripped.length < 32) return false;
  if (/^[A-Za-z]+$/.test(stripped)) {
    const lower = stripped.toLowerCase();
    if (GREETING_WORDS.has(lower)) return false;
    if (
      /^[A-Z][a-z]+$/.test(stripped) &&
      stripped.length >= 3 &&
      stripped.length < 24 &&
      (/[.,:;]$/.test(line) || prevLineLength < 48)
    ) {
      return false;
    }
    if (stripped.length < 16 && prevLineLength < 45) return false;
  }
  return true;
}

function stripTrailingPunctuation(query: string): string {
  return query.replace(TRAILING_PUNCT_REGEX, '');
}

export function unwrapHumocoUriLines(input: string, schemePrefix: string): string {
  const lower = input.toLowerCase();
  const idx = lower.indexOf(schemePrefix.toLowerCase());
  if (idx === -1) return input;
  const slice = input.slice(idx);
  const lines = slice.split(/\r?\n/);
  let joined = (lines[0] || '').trim();
  let prevLineLen = joined.length;
  for (let i = 1; i < lines.length; i++) {
    const line = lines[i].trim();
    if (isWrappedUriContinuationLine(line, prevLineLen)) {
      joined += line;
      prevLineLen = line.length;
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

export type ParsedHumocoInput =
  | { kind: 'payment'; payment: ParsedPaymentRequest; uri: string }
  | { kind: 'card'; card: ParsedCardUri; payload: string }
  | { kind: 'contact'; contact: ParsedContactCard }
  | { kind: 'did'; did: string };

const DEEPLINK_PATTERN =
  /humoco:\/\/(recovery-reply|kit|recovery-req|card|pay|contact)[^\s"'`<>„“”‚‘’«»‹›)]+/i;

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

function stripSurroundingWrappers(value: string): string {
  return value
    .trim()
    .replace(/^["'`„“”‚‘’«»‹›<(\[]+/, '')
    .replace(/["'`„“”‚‘’«»‹›>)\],.;:!?]+$/, '')
    .trim();
}

function canonicalizeUserIdCandidate(value: string): string {
  const cleaned = stripSurroundingWrappers(value);
  if (!cleaned) return '';

  const atIdx = cleaned.lastIndexOf('@');
  if (atIdx === -1) {
    if (/^did:key:/i.test(cleaned)) {
      return `did:key:${cleaned.slice(8)}`;
    }
    return cleaned;
  }

  const hrp = cleaned.slice(0, atIdx);
  const didPart = cleaned.slice(atIdx + 1);
  const normDid = /^did:key:/i.test(didPart) ? `did:key:${didPart.slice(8)}` : didPart;
  const colonIdx = hrp.indexOf(':');
  if (colonIdx <= 0) {
    return `${hrp}@${normDid}`;
  }
  const prefix = hrp.slice(0, colonIdx).toLowerCase();
  const checksum = hrp.slice(colonIdx + 1);
  return `${prefix}:${checksum}@${normDid}`;
}

export function isValidUserIdFormat(id: string): boolean {
  if (/^did:key:z[a-zA-Z0-9]+$/.test(id)) return true;
  const atIdx = id.indexOf('@');
  if (atIdx <= 0) return false;
  const hrp = id.slice(0, atIdx);
  const didPart = id.slice(atIdx + 1);
  if (!/^did:key:z[a-zA-Z0-9]+$/.test(didPart)) return false;
  return /^[a-z0-9](?:[a-z0-9]|-(?=[a-z0-9])){0,62}:[1-9A-HJ-NP-Za-km-z]{2,8}$/.test(hrp);
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
      const rawDid = searchParams.get('did');
      const did = rawDid ? canonicalizeUserIdCandidate(rawDid) : '';
      const name = searchParams.get('name')?.trim() || undefined;
      const rawSync = searchParams.getAll('sync');
      const syncUrls = rawSync
        .map(s => s.trim())
        .filter(s => s.length > 0 && (!/^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//.test(s) || /^https?:\/\//i.test(s)))
        .map(s => normalizeSyncUrlInput(s))
        .filter(Boolean)
        .slice(0, 2);

      if (did && isValidUserIdFormat(did)) {
        return {
          did,
          name,
          syncUrls: syncUrls.length > 0 ? syncUrls : undefined,
        };
      }
    } catch {
      // Malformed humoco://contact URI fails closed below
    }
    return null;
  }

  // Normalize line wraps placed directly between '@' and 'did:key:' without ever stripping broken prefixes.
  const normalizedInput = input.replace(/@[ \t]*\r?\n[ \t]*(?=did:key:)/gi, '@');
  const standalone = canonicalizeUserIdCandidate(normalizedInput);
  // Check if the trimmed/unwrapped input is a standalone DID or subaccount ID
  if (standalone && !/\s/.test(standalone) && (standalone.startsWith('did:') || standalone.includes('@did:'))) {
    return isValidUserIdFormat(standalone) ? { did: standalone } : null;
  }

  // Fallback: extract embedded user ID (including any prefix before @did:key:) from chat/email text.
  // Never strip an invalid prefix or whitespace-separated '@' prefix and fall back to the root DID.
  const trimmedNormalized = normalizedInput.trim();
  const embeddedDidMatch = trimmedNormalized.match(
    /(?:^|[\s"'`„“”‚‘’«»‹›<(\[])((?:[^\s"'`„“”‚‘’«»‹›<(\[\]]+@)?did:key:[a-zA-Z0-9]+)/i
  );
  if (embeddedDidMatch?.[1]) {
    const candidate = canonicalizeUserIdCandidate(embeddedDidMatch[1]);
    if (!candidate) return null;
    const tokenStart =
      (embeddedDidMatch.index ?? 0) + (embeddedDidMatch[0].length - embeddedDidMatch[1].length);
    const beforeToken = trimmedNormalized.slice(0, tokenStart);
    if (!candidate.includes('@') && /@[ \t\r\n]*$/.test(beforeToken)) {
      return null;
    }
    return isValidUserIdFormat(candidate) ? { did: candidate } : null;
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
    const rawTo = searchParams.get('to');
    const to = rawTo ? canonicalizeUserIdCandidate(rawTo) : '';

    if (!to || !isValidUserIdFormat(to)) {
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

function decodeBase64UrlToUtf8(rawBase64: string): string | undefined {
  try {
    const b64 = rawBase64.replace(/-/g, '+').replace(/_/g, '/');
    const padLen = (4 - (b64.length % 4)) % 4;
    const padded = b64 + '='.repeat(padLen);
    const binStr = typeof globalThis.atob === 'function' ? globalThis.atob(padded) : '';
    if (!binStr) return undefined;
    const bytes = new Uint8Array(binStr.length);
    for (let i = 0; i < binStr.length; i++) {
      bytes[i] = binStr.charCodeAt(i);
    }
    return new TextDecoder().decode(bytes);
  } catch {
    return undefined;
  }
}

/**
 * Extracts a supported `humoco://` deep link from raw dropped/pasted text,
 * unwrapping hard email line breaks and stripping trailing chat punctuation.
 */
export function extractHumocoDeepLink(rawText?: string | null): string | null {
  if (!rawText || typeof rawText !== 'string') return null;
  const trimmed = rawText.trim();
  if (!trimmed) return null;

  const lower = trimmed.toLowerCase();
  const schemeMatch = lower.match(/humoco:\/\/(recovery-reply|kit|recovery-req|card|pay|contact)\b/);
  const candidate = schemeMatch
    ? unwrapHumocoUriLines(trimmed, schemeMatch[0])
    : trimmed;

  const match = candidate.match(DEEPLINK_PATTERN);
  if (match?.[0]) {
    return stripTrailingPunctuation(match[0]);
  }
  return null;
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

    const envelopeJson = decodeBase64UrlToUtf8(env);

    return {
      envBase64: env,
      envelopeJson,
    };
  } catch {
    return null;
  }
}

/**
 * Extracts a canonical CardEnvelope payload string (`humoco://card?env=...` or JSON envelope)
 * from raw text, email messages, or file contents.
 */
export function extractCardPayload(raw: string): string | null {
  if (!raw || typeof raw !== 'string') return null;
  const trimmed = raw.trim();
  if (!trimmed) return null;

  const parsedCard = parseCardUri(trimmed);
  if (parsedCard?.envBase64) {
    return `humoco://card?env=${parsedCard.envBase64}`;
  }

  const firstBrace = trimmed.indexOf('{');
  const lastBrace = trimmed.lastIndexOf('}');
  if (firstBrace !== -1 && lastBrace > firstBrace) {
    const candidate = trimmed.slice(firstBrace, lastBrace + 1);
    try {
      const parsed = JSON.parse(candidate) as Record<string, unknown> | null;
      if (
        typeof parsed === 'object' &&
        parsed !== null &&
        'card_wire' in parsed &&
        typeof parsed.card_wire === 'string' &&
        parsed.card_wire.trim().length > 0
      ) {
        return candidate;
      }
    } catch {
      return null;
    }
  }
  return null;
}

/**
 * Central input normalizer/parser for Human Money URIs (`humoco://pay`, `humoco://card`,
 * `humoco://contact`), raw JSON CardEnvelopes, and standalone or chat-embedded User IDs.
 */
export function parseHumocoInput(raw: string): ParsedHumocoInput | null {
  if (!raw || typeof raw !== 'string') return null;
  const trimmed = raw.trim();
  if (!trimmed) return null;

  const payment = parsePaymentRequest(trimmed);
  if (payment) {
    const deepLink = extractHumocoDeepLink(trimmed) || buildPaymentRequestUri(payment);
    return { kind: 'payment', payment, uri: deepLink };
  }

  const parsedCardUri = parseCardUri(trimmed);
  if (parsedCardUri) {
    const payload = `humoco://card?env=${parsedCardUri.envBase64}`;
    return { kind: 'card', card: parsedCardUri, payload };
  }

  const cardJsonPayload = extractCardPayload(trimmed);
  if (cardJsonPayload && cardJsonPayload.startsWith('{')) {
    return {
      kind: 'card',
      card: { envBase64: '', envelopeJson: cardJsonPayload },
      payload: cardJsonPayload,
    };
  }

  const contact = parseContactCard(trimmed);
  if (contact) {
    const hasContactUri = CONTACT_URI_REGEX.test(unwrapHumocoUriLines(trimmed, 'humoco://contact?'));
    if (hasContactUri) {
      return { kind: 'contact', contact };
    }
    return { kind: 'did', did: contact.did };
  }

  return null;
}

/**
 * Extracts and validates a canonical User ID (`did:key:z...` or `prefix:checksum@did:key:z...`)
 * from URIs, contact links, or chat/email paste.
 * Signed card envelopes require canonical Ed25519 verification via `marketService.resolveCardRecipient`.
 */
export function normalizeUserIdInput(raw: string): string | null {
  const parsed = parseHumocoInput(raw);
  if (!parsed) return null;
  switch (parsed.kind) {
    case 'payment':
      return parsed.payment.to;
    case 'contact':
      return parsed.contact.did;
    case 'card':
      return null;
    case 'did':
      return parsed.did;
  }
}

/**
 * Normalizes user-entered or pasted Sync Service URLs (extracts embedded http/https URLs from chat text,
 * enforces https except loopback, strips query/fragment, appends `/sync.php` cleanly).
 */
export function normalizeSyncUrlInput(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return '';

  const embeddedUrl = trimmed.match(/https?:\/\/[^\s<>"'`„“”‚‘’«»‹›]+/i);
  let s = embeddedUrl ? embeddedUrl[0].replace(/[.,;:!?)\]"'`„“”‚‘’«»‹›]+$/, '') : trimmed;

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

