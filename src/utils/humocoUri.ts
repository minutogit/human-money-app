/**
 * Utilities for building and parsing Human Money custom desktop URIs and text templates.
 * 
 * Supports:
 * - Contact cards: humoco://contact?did=...&name=...
 * - Payment requests: humoco://pay?to=...&amount=...&unit=...&std=...&note=...
 * - Market contact cards: humoco://card?env=...
 */

export interface ContactCardParams {
  did: string;
  name?: string;
}

export interface ParsedContactCard {
  did: string;
  name?: string;
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

export interface ParsedCardUri {
  envBase64: string;
  envelopeJson?: string;
}

/**
 * Builds a friendly, human-readable contact card text block containing a humoco://contact URI.
 */
export function buildContactCardText({ did, name }: ContactCardParams): string {
  const cleanDid = did.trim();
  const cleanName = name?.trim();

  const params = new URLSearchParams();
  params.set('did', cleanDid);
  if (cleanName) {
    params.set('name', cleanName);
  }

  const uri = `humoco://contact?${params.toString().replace(/\+/g, '%20')}`;

  const lines: string[] = ['Human Money Kontaktkarte:'];
  if (cleanName) {
    lines.push(`Name: ${cleanName}`);
  }
  lines.push(`DID: ${cleanDid}`);
  lines.push('');
  lines.push(uri);

  return lines.join('\n');
}

/**
 * Parses a contact card from raw text.
 * 
 * Handles:
 * 1. Embedded humoco://contact?did=...&name=... inside multiline emails/chat messages
 * 2. Standalone DID string (did:key:... or prefix:tag@did:key:...)
 */
export function parseContactCard(input: string): ParsedContactCard | null {
  if (!input || typeof input !== 'string') return null;

  const match = input.match(CONTACT_URI_REGEX);
  if (match && match[1]) {
    try {
      const searchParams = new URLSearchParams(match[1]);
      const did = searchParams.get('did')?.trim();
      const name = searchParams.get('name')?.trim() || undefined;

      if (did && (did.startsWith('did:') || did.includes('@did:'))) {
        return { did, name };
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

  return null;
}

/**
 * Builds a human-readable payment request message for email/messenger containing a humoco://pay URI.
 */
export function buildPaymentRequestText(params: PaymentRequestParams): string {
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

  const uri = `humoco://pay?${searchParams.toString().replace(/\+/g, '%20')}`;

  const lines: string[] = ['Human Money Zahlungsanforderung:'];
  if (cleanName && cleanTo) {
    lines.push(`Empfänger: ${cleanName} (${cleanTo})`);
  } else if (cleanName) {
    lines.push(`Empfänger: ${cleanName}`);
  } else if (cleanTo) {
    lines.push(`Empfänger: ${cleanTo}`);
  }

  if (amountStr && cleanUnit) {
    lines.push(`Betrag: ${amountStr} ${cleanUnit}`);
  } else if (amountStr) {
    lines.push(`Betrag: ${amountStr}`);
  } else if (cleanUnit) {
    lines.push(`Währung: ${cleanUnit}`);
  }

  if (cleanNote) {
    lines.push(`Verwendungszweck: ${cleanNote}`);
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

  const match = input.match(PAY_URI_REGEX);
  if (!match || !match[1]) return null;

  try {
    const searchParams = new URLSearchParams(match[1]);
    const to = searchParams.get('to')?.trim();

    if (!to || (!to.startsWith('did:') && !to.includes('@did:'))) {
      return null;
    }

    const name = searchParams.get('name')?.trim() || undefined;
    const standardUuid = searchParams.get('std')?.trim() || searchParams.get('standardUuid')?.trim() || undefined;
    const standardName = searchParams.get('stdName')?.trim() || searchParams.get('standardName')?.trim() || undefined;
    const unit = searchParams.get('unit')?.trim() || undefined;
    const amount = searchParams.get('amount')?.trim() || undefined;
    const note = searchParams.get('note')?.trim() || searchParams.get('notes')?.trim() || undefined;

    const testParam = searchParams.get('test') ?? searchParams.get('isTest');
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
 * Builds a humoco://card URI from a base64-encoded CardEnvelope JSON.
 */
export function buildCardUri(envBase64: string): string {
  return `humoco://card?env=${envBase64.trim()}`;
}

/**
 * Parses a humoco://card URI from arbitrary text.
 * 
 * Extracts humoco://card?env=... and decodes the CardEnvelope JSON if possible.
 */
export function parseCardUri(input: string): ParsedCardUri | null {
  if (!input || typeof input !== 'string') return null;

  const match = input.match(CARD_URI_REGEX);
  if (!match || !match[1]) return null;

  try {
    const searchParams = new URLSearchParams(match[1]);
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

