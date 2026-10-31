import { describe, it, expect } from 'vitest';
import {
  buildContactCardText,
  parseContactCard,
  buildPaymentRequestText,
  parsePaymentRequest,
} from '../humocoUri';

describe('humocoUri Utilities', () => {
  const sampleDid = 'did:key:z6MkuT52y5aBqB9R5e6v18c8P4h5M8Z7v';
  const sampleSai = 'alice:work@did:key:z6MkuT52y5aBqB9R5e6v18c8P4h5M8Z7v';

  describe('buildContactCardText', () => {
    it('builds a contact card with name and did', () => {
      const text = buildContactCardText({ did: sampleDid, name: 'Alice Müller' });
      expect(text).toContain('Human Money Kontaktkarte:');
      expect(text).toContain('Name: Alice Müller');
      expect(text).toContain(`DID: ${sampleDid}`);
      expect(text).toContain(`humoco://contact?did=${encodeURIComponent(sampleDid)}&name=${encodeURIComponent('Alice Müller')}`);
    });

    it('builds a contact card without name', () => {
      const text = buildContactCardText({ did: sampleDid });
      expect(text).toContain('Human Money Kontaktkarte:');
      expect(text).not.toContain('Name:');
      expect(text).toContain(`DID: ${sampleDid}`);
      expect(text).toContain(`humoco://contact?did=${encodeURIComponent(sampleDid)}`);
    });
  });

  describe('parseContactCard', () => {
    it('parses humoco://contact embedded in multiline text', () => {
      const email = `
        Hallo Basti!
        Hier ist meine Kontaktkarte für Human Money:
        
        humoco://contact?did=${encodeURIComponent(sampleDid)}&name=${encodeURIComponent('Dr. René Schäfer')}
        
        Bitte füge mich hinzu!
        Viele Grüße, René
      `;

      const result = parseContactCard(email);
      expect(result).not.toBeNull();
      expect(result?.did).toBe(sampleDid);
      expect(result?.name).toBe('Dr. René Schäfer');
    });

    it('parses humoco://contact without name', () => {
      const raw = `Check this: humoco://contact?did=${encodeURIComponent(sampleDid)}`;
      const result = parseContactCard(raw);
      expect(result).toEqual({ did: sampleDid, name: undefined });
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

    it('handles alternate parameter names standardUuid and notes', () => {
      const raw = `humoco://pay?to=${encodeURIComponent(sampleDid)}&standardUuid=custom-std&notes=Miete%20Werkstatt&isTest=true`;
      const result = parsePaymentRequest(raw);
      expect(result).toEqual({
        to: sampleDid,
        name: undefined,
        standardUuid: 'custom-std',
        standardName: undefined,
        unit: undefined,
        amount: undefined,
        note: 'Miete Werkstatt',
        isTest: true,
      });
    });

    it('returns null if to is missing or not a did', () => {
      expect(parsePaymentRequest('humoco://pay?amount=50')).toBeNull();
      expect(parsePaymentRequest('humoco://pay?to=invalid-recipient&amount=50')).toBeNull();
      expect(parsePaymentRequest('Just text without uri')).toBeNull();
    });
  });
});
