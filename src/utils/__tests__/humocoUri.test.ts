import { describe, it, expect } from 'vitest';
import { Buffer } from 'node:buffer';
import {
  buildContactCardText,
  parseContactCard,
  buildPaymentRequestText,
  parsePaymentRequest,
  buildCardUri,
  parseCardUri,
} from '../humocoUri';

describe('humocoUri Utilities', () => {
  const sampleDid = 'did:key:z6MkuT52y5aBqB9R5e6v18c8P4h5M8Z7v';
  const sampleSai = 'alice:work@did:key:z6MkuT52y5aBqB9R5e6v18c8P4h5M8Z7v';

  describe('buildContactCardText', () => {
    it('outputs only the compact humoco://contact URI with name, did, and syncUrls', () => {
      const text = buildContactCardText({
        did: sampleDid,
        name: 'Alice Müller',
        syncUrls: ['https://sync1.example.org/sync.php', 'https://sync2.example.org/sync.php'],
      });
      expect(text).not.toContain('Human Money Kontakt:');
      expect(text).not.toContain('\n');
      expect(text).toBe(`humoco://contact?did=${encodeURIComponent(sampleDid)}&name=${encodeURIComponent('Alice Müller')}&sync=sync1.example.org&sync=sync2.example.org`);
    });

    it('outputs only the humoco://contact URI without name', () => {
      const text = buildContactCardText({ did: sampleDid });
      expect(text).not.toContain('Human Money Kontakt:');
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

  describe('buildCardUri and parseCardUri', () => {
    const sampleEnvelopeJson = JSON.stringify({
      card_jws: 'eyJhbGciOiJFZERTQSIsInR5cCI6ImhtYy1jYXJkK2p3dCJ9.payload.sig',
      hops: 1,
    });
    // encode in base64
    const sampleB64 = Buffer.from(sampleEnvelopeJson, 'utf8').toString('base64');

    it('builds a humoco://card URI with base64 envelope', () => {
      const uri = buildCardUri(sampleB64);
      expect(uri).toBe(`humoco://card?env=${sampleB64}`);
    });

    it('parses a humoco://card URI and decodes envelope JSON', () => {
      const uri = `humoco://card?env=${sampleB64}`;
      const result = parseCardUri(uri);
      expect(result).not.toBeNull();
      expect(result?.envBase64).toBe(sampleB64);
      expect(result?.envelopeJson).toBe(sampleEnvelopeJson);
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
  });
});
