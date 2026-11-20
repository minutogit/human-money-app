import { describe, it, expect } from 'vitest';
import { Buffer } from 'node:buffer';
import {
  buildContactUri,
  parseContactCard,
  buildPaymentRequestText,
  parsePaymentRequest,
  parseCardUri,
  parseHumocoInput,
  normalizeUserIdInput,
  extractCardPayload,
  isValidUserIdFormat,
  extractHumocoDeepLink,
} from '../humocoUri';

describe('humocoUri Utilities', () => {
  const sampleDid = 'did:key:z6MkuT52y5aBqB9R5e6v18c8P4h5M8Z7v';
  const sampleSai = 'alice:work@did:key:z6MkuT52y5aBqB9R5e6v18c8P4h5M8Z7v';

  describe('buildContactUri', () => {
    it('outputs a compact humoco://contact URI encoding parentheses safely', () => {
      const text = buildContactUri({
        did: sampleDid,
        name: 'Alice (Workshop)',
        syncUrls: ['https://sync1.example.org/sync.php', 'https://sync2.example.org/sync.php'],
      });
      expect(text).not.toContain('\n');
      expect(text).not.toContain('(');
      expect(text).not.toContain(')');
      expect(text).toBe(
        `humoco://contact?did=${encodeURIComponent(sampleDid)}&name=Alice%20%28Workshop%29&sync=sync1.example.org&sync=sync2.example.org`
      );
    });

    it('outputs a compact humoco://contact URI without name', () => {
      const text = buildContactUri({ did: sampleDid });
      expect(text).toBe(`humoco://contact?did=${encodeURIComponent(sampleDid)}`);
    });
  });

  describe('parseContactCard', () => {
    it('parses humoco://contact embedded in multiline text with sync parameters', () => {
      const email = `
        Hallo Basti!
        Hier ist meine Kontaktkarte für Human Money:
        
        humoco://contact?did=${encodeURIComponent(sampleDid)}&name=${encodeURIComponent('Dr. René Schäfer')}&sync=${encodeURIComponent('https://node1.humoco.org/sync.php')}&sync=${encodeURIComponent('https://node2.humoco.org/sync.php')}
        
        Bitte füge mich hinzu!
        Viele Grüße, René
      `;

      const result = parseContactCard(email);
      expect(result).not.toBeNull();
      expect(result?.did).toBe(sampleDid);
      expect(result?.name).toBe('Dr. René Schäfer');
      expect(result?.syncUrls).toEqual([
        'https://node1.humoco.org/sync.php',
        'https://node2.humoco.org/sync.php',
      ]);
    });

    it('parses humoco://contact without name and without sync', () => {
      const raw = `Check this: humoco://contact?did=${encodeURIComponent(sampleDid)}`;
      const result = parseContactCard(raw);
      expect(result).toEqual({ did: sampleDid, name: undefined, syncUrls: undefined });
    });

    it('limits parsed sync URLs to maximum 2 and filters non-http(s)', () => {
      const raw = `humoco://contact?did=${encodeURIComponent(sampleDid)}&sync=ftp://bad.com&sync=sync1.org&sync=http://127.0.0.1:8080&sync=sync3.org`;
      const result = parseContactCard(raw);
      expect(result?.syncUrls).toEqual([
        'https://sync1.org/sync.php',
        'http://127.0.0.1:8080/sync.php',
      ]);
    });

    it('parses a standalone did:key string with surrounding whitespace', () => {
      const raw = `   \n  ${sampleDid}  \t `;
      const result = parseContactCard(raw);
      expect(result).toEqual({ did: sampleDid });
    });

    it('parses a standalone SAI did string', () => {
      const raw = ` ${sampleSai} `;
      const result = parseContactCard(raw);
      expect(result).toEqual({ did: sampleSai });
    });

    it('extracts embedded did:key or SAI did from chat message with surrounding text', () => {
      const chatText = `Hi! Here is my ID:\n"${sampleDid}"\nSee you tomorrow!`;
      expect(parseContactCard(chatText)).toEqual({ did: sampleDid });

      const saiChatText = `My work account is ${sampleSai} thanks!`;
      expect(parseContactCard(saiChatText)).toEqual({ did: sampleSai });
    });

    it('strips surrounding quotes/backticks on standalone IDs and rejects malformed subaccount prefixes instead of falling back to root DID', () => {
      expect(parseContactCard(`"kasse:7x9@${sampleDid}"`)).toEqual({
        did: `kasse:7x9@${sampleDid}`,
      });
      expect(parseContactCard(`Bitte zahle an \`kasse:7x9@${sampleDid}\``)).toEqual({
        did: `kasse:7x9@${sampleDid}`,
      });
      expect(parseContactCard(`Bitte zahle an kasse@${sampleDid}`)).toBeNull();
      expect(parseContactCard(`"${sampleDid}"`)).toEqual({ did: sampleDid });
    });

    it('returns null for arbitrary text or invalid did', () => {
      expect(parseContactCard('')).toBeNull();
      expect(parseContactCard('random chatter without any did')).toBeNull();
      expect(parseContactCard('humoco://contact?did=not-a-did')).toBeNull();
      expect(parseContactCard('https://example.com/not-did')).toBeNull();
    });
  });

  describe('buildPaymentRequestText', () => {
    it('builds a complete payment request with all fields', () => {
      const text = buildPaymentRequestText({
        to: sampleDid,
        name: 'Biogarten Imkerei',
        standardUuid: 'std-minuto-001',
        standardName: 'Minuto',
        unit: 'Minuten',
        amount: '15.5',
        note: '2 Gläser Honig & Propolis',
        isTest: true,
      });

      expect(text).toContain('Human Money Zahlungsanforderung:');
      expect(text).toContain(`Empfänger: Biogarten Imkerei (${sampleDid})`);
      expect(text).toContain('Betrag: 15.5 Minuten');
      expect(text).toContain('Verwendungszweck: 2 Gläser Honig & Propolis');
      expect(text).toContain('humoco://pay?');
      expect(text).toContain(`to=${encodeURIComponent(sampleDid)}`);
      expect(text).toContain(`name=${encodeURIComponent('Biogarten Imkerei')}`);
      expect(text).toContain('amount=15.5');
      expect(text).toContain('unit=Minuten');
      expect(text).toContain('std=std-minuto-001');
      expect(text).toContain('stdName=Minuto');
      expect(text).toContain(`note=${encodeURIComponent('2 Gläser Honig & Propolis')}`);
      expect(text).toContain('test=true');
    });

    it('builds a minimal payment request', () => {
      const text = buildPaymentRequestText({
        to: sampleDid,
      });

      expect(text).toContain('Human Money Zahlungsanforderung:');
      expect(text).toContain(`Empfänger: ${sampleDid}`);
      expect(text).not.toContain('Betrag:');
      expect(text).not.toContain('Verwendungszweck:');
      expect(text).toContain(`humoco://pay?to=${encodeURIComponent(sampleDid)}`);
    });
  });

  describe('parsePaymentRequest', () => {
    it('extracts payment request from multiline chat message with umlauts', () => {
      const chatMessage = `
        Hi Alice,
        bitte überweise mir die 15 Minuto für den Honig:
        humoco://pay?to=${encodeURIComponent(sampleDid)}&name=${encodeURIComponent('Bob Imker')}&amount=15&unit=Minuto&std=minuto-uuid&note=${encodeURIComponent('2 Gläser Sommerblüte')}&test=true
        Danke dir!
      `;

      const result = parsePaymentRequest(chatMessage);
      expect(result).not.toBeNull();
      expect(result?.to).toBe(sampleDid);
      expect(result?.name).toBe('Bob Imker');
      expect(result?.amount).toBe('15');
      expect(result?.unit).toBe('Minuto');
      expect(result?.standardUuid).toBe('minuto-uuid');
      expect(result?.note).toBe('2 Gläser Sommerblüte');
      expect(result?.isTest).toBe(true);
    });

    it('ignores non-canonical legacy parameter names standardUuid and notes under zero-compat', () => {
      const raw = `humoco://pay?to=${encodeURIComponent(sampleDid)}&standardUuid=custom-std&notes=Miete%20Werkstatt&isTest=true`;
      const result = parsePaymentRequest(raw);
      expect(result).toEqual({
        to: sampleDid,
        name: undefined,
        standardUuid: undefined,
        standardName: undefined,
        unit: undefined,
        amount: undefined,
        note: undefined,
        isTest: undefined,
      });
    });

    it('returns null if to is missing or not a did', () => {
      expect(parsePaymentRequest('humoco://pay?amount=50')).toBeNull();
      expect(parsePaymentRequest('humoco://pay?to=invalid-recipient&amount=50')).toBeNull();
      expect(parsePaymentRequest('Just text without uri')).toBeNull();
    });
  });

  describe('parseCardUri', () => {
    const sampleEnvelopeJson = JSON.stringify({
      card_wire: 'eyJhbGciOiJFZERTQSIsInR5cCI6ImhtYy1jYXJkK2p3dCJ9.payload.sig',
      hops: 1,
    });
    // encode in base64
    const sampleB64 = Buffer.from(sampleEnvelopeJson, 'utf8').toString('base64');

    it('parses a humoco://card URI and decodes envelope JSON', () => {
      const uri = `humoco://card?env=${sampleB64}`;
      const result = parseCardUri(uri);
      expect(result).not.toBeNull();
      expect(result?.envBase64).toBe(sampleB64);
      expect(result?.envelopeJson).toBe(sampleEnvelopeJson);
    });

    it('unwraps line-wrapped humoco:// URIs from email clients', () => {
      const fullUri = `humoco://contact?did=${encodeURIComponent(sampleDid)}&name=Alice%20%28Shop%29`;
      const wrapped = `${fullUri.slice(0, 32)}\n${fullUri.slice(32, 58)}\n  ${fullUri.slice(58)}`;
      const result = parseContactCard(wrapped);
      expect(result).toEqual({
        did: sampleDid,
        name: 'Alice (Shop)',
        syncUrls: undefined,
      });
    });

    it('parses humoco://card embedded in chat or email text', () => {
      const message = `
        Hey check out my contact card:
        humoco://card?env=${sampleB64}
        See you on the marketplace!
      `;
      const result = parseCardUri(message);
      expect(result).not.toBeNull();
      expect(result?.envBase64).toBe(sampleB64);
      expect(result?.envelopeJson).toBe(sampleEnvelopeJson);
    });

    it('returns null for empty or invalid card URI', () => {
      expect(parseCardUri('')).toBeNull();
      expect(parseCardUri('humoco://card')).toBeNull();
      expect(parseCardUri('humoco://card?other=param')).toBeNull();
      expect(parseCardUri('https://example.com/card?env=123')).toBeNull();
    });

    it('handles trailing punctuation, case-insensitive schemes, and German comma decimals in chat paste', () => {
      const contactRes = parseContactCard(`Bitte nimm (HUMOCO://CONTACT?did=${encodeURIComponent(sampleDid)}).`);
      expect(contactRes?.did).toBe(sampleDid);

      const payRes = parsePaymentRequest(
        `Überweise bitte an HUMOCO://PAY?to=${encodeURIComponent(sampleDid)}&amount=15,50.`
      );
      expect(payRes?.to).toBe(sampleDid);
      expect(payRes?.amount).toBe('15.50');

      const cardRes = parseCardUri(`Meine Karte: humoco://card?env=${sampleB64}!`);
      expect(cardRes?.envBase64).toBe(sampleB64);
    });

    it('does not append trailing prose greetings like Danke! to multiline humoco URIs', () => {
      const chatText = [
        `humoco://pay?to=${encodeURIComponent(sampleDid)}&amount=25`,
        'Danke!',
      ].join('\n');
      const parsed = parsePaymentRequest(chatText);
      expect(parsed?.to).toBe(sampleDid);
      expect(parsed?.amount).toBe('25');

      const cardText = [
        `humoco://card?env=${sampleB64}`,
        'Danke!',
      ].join('\n');
      const parsedCard = parseCardUri(cardText);
      expect(parsedCard?.envBase64).toBe(sampleB64);
    });
  });

  describe('centralized normalizer & validators', () => {
    const toBase64Url = (obj: unknown) =>
      Buffer.from(JSON.stringify(obj), 'utf8')
        .toString('base64')
        .replace(/\+/g, '-')
        .replace(/\//g, '_')
        .replace(/=+$/, '');

    it('isValidUserIdFormat accepts valid DIDs and SAIs and rejects malformed inputs', () => {
      expect(isValidUserIdFormat(sampleDid)).toBe(true);
      expect(isValidUserIdFormat(sampleSai)).toBe(true);
      expect(isValidUserIdFormat('kasse:7x9@did:key:z6MkuT52y5aBqB9R5e6v18c8P4h5M8Z7v')).toBe(true);

      expect(isValidUserIdFormat('')).toBe(false);
      expect(isValidUserIdFormat('did:key:short')).toBe(false);
      expect(isValidUserIdFormat(`kasse@${sampleDid}`)).toBe(false);
      expect(isValidUserIdFormat(`alice:work@did:key: z6MkuT52y5aBqB9R5e6v18c8P4h5M8Z7v`)).toBe(false);
    });

    it('extractHumocoDeepLink unwraps hard line breaks and strips trailing punctuation', () => {
      const wrapped = `Hier mein Link:\nhumoco://contact?did=${encodeURIComponent(sampleDid)}\n&name=Alice.`;
      expect(extractHumocoDeepLink(wrapped)).toBe(
        `humoco://contact?did=${encodeURIComponent(sampleDid)}&name=Alice`
      );
      expect(extractHumocoDeepLink('no humoco scheme here')).toBeNull();
      expect(extractHumocoDeepLink(null)).toBeNull();
    });

    it('extractCardPayload extracts card URIs and JSON envelopes while rejecting arbitrary HTTP env links', () => {
      const envJson = JSON.stringify({ card_wire: 'hdr.payload.sig', did: sampleDid });
      const envB64Url = toBase64Url(JSON.parse(envJson));
      const multilineUri = `Bitte öffnen:\nhumoco://card?env=${envB64Url.slice(0, 20)}\n${envB64Url.slice(20)}!`;
      expect(extractCardPayload(multilineUri)).toBe(`humoco://card?env=${envB64Url}`);

      const wrappedJson = `Kontaktkarte:\n${envJson}\nDanke!`;
      expect(extractCardPayload(wrappedJson)).toBe(envJson);

      expect(extractCardPayload('https://example.com/card?env=123')).toBeNull();
    });

    it('parseHumocoInput discriminates payment, card, contact, and embedded DID inputs', () => {
      const payRes = parseHumocoInput(`Bitte zahlen: humoco://pay?to=${encodeURIComponent(sampleDid)}&amount=12.50&note=Honig`);
      expect(payRes?.kind).toBe('payment');
      if (payRes?.kind === 'payment') {
        expect(payRes.payment.to).toBe(sampleDid);
        expect(payRes.payment.amount).toBe('12.50');
      }

      const contactRes = parseHumocoInput(`humoco://contact?did=${encodeURIComponent(sampleDid)}&name=Alice`);
      expect(contactRes?.kind).toBe('contact');
      if (contactRes?.kind === 'contact') {
        expect(contactRes.contact.did).toBe(sampleDid);
        expect(contactRes.contact.name).toBe('Alice');
      }

      const didRes = parseHumocoInput(`Meine ID im Chat lautet "${sampleSai}", bitte melden.`);
      expect(didRes).toEqual({ kind: 'did', did: sampleSai });

      const header = toBase64Url({ alg: 'EdDSA' });
      const payload = toBase64Url({ did: sampleDid });
      const cardJson = JSON.stringify({ card_wire: `${header}.${payload}.sig` });
      const cardB64 = toBase64Url(JSON.parse(cardJson));
      const cardUriRes = parseHumocoInput(`humoco://card?env=${cardB64}`);
      expect(cardUriRes?.kind).toBe('card');
      if (cardUriRes?.kind === 'card') {
        expect(cardUriRes.payload).toBe(`humoco://card?env=${cardB64}`);
        expect('did' in cardUriRes).toBe(false);
      }

      const cardJsonRes = parseHumocoInput(cardJson);
      expect(cardJsonRes?.kind).toBe('card');
      if (cardJsonRes?.kind === 'card') {
        expect(cardJsonRes.payload).toBe(cardJson);
        expect('did' in cardJsonRes).toBe(false);
      }

      expect(parseHumocoInput('plain text without humoco uri')).toBeNull();
      expect(parseHumocoInput(`invalid@${sampleDid}`)).toBeNull();
    });

    it('normalizeUserIdInput extracts canonical user IDs and leaves signed card envelopes to backend verification', () => {
      const header = toBase64Url({ alg: 'EdDSA' });
      const payload = toBase64Url({ did: sampleDid });
      const cardJson = JSON.stringify({ card_wire: `${header}.${payload}.sig` });
      const cardB64 = toBase64Url(JSON.parse(cardJson));

      expect(normalizeUserIdInput(`humoco://pay?to=${encodeURIComponent(sampleDid)}&amount=5`)).toBe(sampleDid);
      expect(normalizeUserIdInput(`humoco://contact?did=${encodeURIComponent(sampleSai)}`)).toBe(sampleSai);
      expect(normalizeUserIdInput(`humoco://card?env=${cardB64}`)).toBeNull();
      expect(normalizeUserIdInput(cardJson)).toBeNull();
      expect(normalizeUserIdInput(`Sende an ${sampleDid}!`)).toBe(sampleDid);
      expect(normalizeUserIdInput(`kasse@${sampleDid}`)).toBeNull();
    });

    it('handles typographic quotes, backticks, @ line breaks, and case-insensitive did:key prefixes while preserving Base58 case', () => {
      const mixedCaseDid = `DID:KEY:z6MkuT52y5aBqB9R5e6v18c8P4h5M8Z7v`;
      expect(parseContactCard(`„${sampleDid}“`)).toEqual({ did: sampleDid });
      expect(parseContactCard(`«${sampleSai}»`)).toEqual({ did: sampleSai });
      expect(parseContactCard(`\`${mixedCaseDid}\``)).toEqual({ did: sampleDid });
      expect(parseContactCard(`Bitte zahlen an ALICE:3K4@DID:KEY:z6MkuT52y5aBqB9R5e6v18c8P4h5M8Z7v!`)).toEqual({
        did: 'alice:3K4@did:key:z6MkuT52y5aBqB9R5e6v18c8P4h5M8Z7v',
      });

      const wrappedPay = `Bitte überweise:\nhumoco://pay?to=alice:work\n@did:key:z6MkuT52y5aBqB9R5e6v18c8P4h5M8Z7v&amount=10`;
      const payParsed = parsePaymentRequest(wrappedPay);
      expect(payParsed?.to).toBe(sampleSai);
      expect(payParsed?.amount).toBe('10');

      const quotedContact = `»HUMOCO://contact?did=${encodeURIComponent(mixedCaseDid)}&name=Bob«`;
      expect(parseContactCard(quotedContact)).toEqual({
        did: sampleDid,
        name: 'Bob',
        syncUrls: undefined,
      });

      // Shortened prefix without checksum must remain rejected (fail-closed)
      expect(parseContactCard(`„kasse@${sampleDid}“`)).toBeNull();
    });

    it('H8/F6: rejects malformed humoco://contact URIs without falling back and rejects legacy card envelopes', () => {
      // Invalid prefix inside humoco://contact URI must fail closed (never fall through to embedded did:key)
      expect(parseContactCard(`humoco://contact?did=bad!prefix@${sampleDid}&name=Evil`)).toBeNull();
      expect(parseContactCard(`humoco://contact?name=NoDid`)).toBeNull();
      // Embedded chat text with punctuation prefix must never fall back to bare root did:key
      expect(parseContactCard(`Bitte an bad!prefix@${sampleDid} zahlen`)).toBeNull();

      // F6: Legacy card+sig or envelope without card_wire must be rejected
      expect(extractCardPayload(JSON.stringify({ card: { name: 'Legacy' }, sig: 'abc' }))).toBeNull();
      expect(extractCardPayload(JSON.stringify({ envelope: { did: sampleDid } }))).toBeNull();
      expect(extractCardPayload(JSON.stringify({ did: sampleDid }))).toBeNull();
      expect(parseHumocoInput(JSON.stringify({ card: { name: 'Legacy' }, sig: 'abc' }))).toBeNull();
    });
  });
});
