import { describe, it, expect, vi } from 'vitest';
import type { TFunction } from 'i18next';
import {
    getMissingProfileHint,
    formatFieldName,
    translateRuleMessage,
    translateGentleHint,
} from '../signatureHints';
import { VoucherStandardDefinition, PublicProfile } from '../../types';

describe('signatureHints', () => {
    describe('formatFieldName', () => {
        it('formats camelCase to Title Case', () => {
            expect(formatFieldName('someFieldName')).toBe('Some Field Name');
            expect(formatFieldName('needs')).toBe('Needs');
        });

        it('formats snake_case and kebab-case to Title Case', () => {
            expect(formatFieldName('some_field_name')).toBe('Some Field Name');
            expect(formatFieldName('some-field-name')).toBe('Some Field Name');
        });
    });

    describe('getMissingProfileHint', () => {
        const mockStandard = (customRules: Record<string, string>): VoucherStandardDefinition => ({
            immutable: {
                identity: {
                    uuid: 'test-uuid',
                    name: 'Test Standard',
                    abbreviation: 'TST',
                    issuerId: '0:is@did:key:z6Mkexample',
                },
                blueprint: {} as VoucherStandardDefinition['immutable']['blueprint'],
                features: {} as VoucherStandardDefinition['immutable']['features'],
                issuance: {
                    allowedSignatureRoles: [],
                } as unknown as VoucherStandardDefinition['immutable']['issuance'],
                customRules: Object.fromEntries(
                    Object.entries(customRules).map(([name, expr]) => [
                        name,
                        { expression: expr, message: 'rule failed' },
                    ])
                ),
            },
            mutable: {} as VoucherStandardDefinition['mutable'],
        });

        const mockProfile: PublicProfile = {
            id: 'did:key:123',
            firstName: 'Alice',
            lastName: 'Smith',
            gender: 'Female',
            email: 'alice@example.com',
            phone: '123456789',
        };

        const rawMockT = (key: string, options?: { defaultValue?: string; field?: string }) => {
            if (key === 'voucher.hint.prefix') return options?.defaultValue || 'This standard likely checks for: ';
            if (key === 'voucher.hint.connectorAnd') return options?.defaultValue || ' and ';
            if (key === 'voucher.hint.connectorComma') return options?.defaultValue || ', ';
            if (key === 'profile.field.gender') return 'Geschlecht';
            if (key === 'profile.field.address') return 'Adresse';
            if (key === 'profile.field.url') return 'Website-Link';
            if (key === 'profile.firstName') return 'Vorname';
            if (key === 'voucher.signRequest.openRuleCheck') return `Prüft: ${options?.field ?? ''}`;
            return options?.defaultValue || key;
        };
        const mockT = rawMockT as unknown as TFunction;

        it('returns null if profile is null', () => {
            const standard = mockStandard({ rule1: 's.details.gender == "Female"' });
            expect(getMissingProfileHint(standard, null, mockT)).toBeNull();
        });

        it('returns null if all required fields are present in profile', () => {
            const standard = mockStandard({ rule1: 's.details.gender == "Female"' });
            expect(getMissingProfileHint(standard, mockProfile, mockT)).toBeNull();
        });

        it('detects missing predefined field and localizes the label', () => {
            const standard = mockStandard({ rule1: 's.details.address != null' });
            const hint = getMissingProfileHint(standard, mockProfile, mockT);
            expect(hint).toBe('This standard likely checks for: Adresse');
        });

        it('detects multiple missing fields and formats them correctly', () => {
            const standard = mockStandard({
                rule1: 's.details.gender != null',
                rule2: 's.details.address != null',
                rule3: 's.details.coordinates != null',
            });
            const incompleteProfile: PublicProfile = {
                id: 'did:key:123',
            };
            const hint = getMissingProfileHint(standard, incompleteProfile, mockT);
            expect(hint).toBe('This standard likely checks for: Geschlecht, Adresse and Coordinates');
        });

        it('handles custom arbitrary fields not in the predefined map', () => {
            const standard = mockStandard({ rule1: 's.details.someCustomField == "xyz"' });
            const hint = getMissingProfileHint(standard, mockProfile, mockT);
            expect(hint).toBe('This standard likely checks for: Some Custom Field');
        });

        it('returns null when customRules is empty', () => {
            const standard = mockStandard({});
            expect(getMissingProfileHint(standard, mockProfile, mockT)).toBeNull();
        });

        it('formats exactly two missing fields with the "and" connector without a comma', () => {
            const standard = mockStandard({
                rule1: 's.details.gender != null',
                rule2: 's.details.address != null',
            });
            const incompleteProfile: PublicProfile = {
                id: 'did:key:123',
            };
            expect(getMissingProfileHint(standard, incompleteProfile, mockT)).toBe(
                'This standard likely checks for: Geschlecht and Adresse'
            );
        });

        it('treats whitespace-only strings and empty address objects as missing while accepting any non-empty address subfield', () => {
            const standard = mockStandard({
                rule1: 's.details.gender != null',
                rule2: 's.details.address != null',
            });
            const whitespaceProfile: PublicProfile = {
                id: 'did:key:123',
                gender: '   ',
                address: { street: '  ', city: '', zipCode: '' },
            };
            expect(getMissingProfileHint(standard, whitespaceProfile, mockT)).toBe(
                'This standard likely checks for: Geschlecht and Adresse'
            );

            const validCityProfile: PublicProfile = {
                id: 'did:key:123',
                gender: 'Female',
                address: { city: 'Berlin' },
            };
            expect(getMissingProfileHint(standard, validCityProfile, mockT)).toBeNull();
        });

        it('deduplicates repeated field references across multiple CEL rules', () => {
            const standard = mockStandard({
                rule1: 's.details.address != null',
                rule2: 'has(s.details.address)',
            });
            expect(getMissingProfileHint(standard, mockProfile, mockT)).toBe(
                'This standard likely checks for: Adresse'
            );
        });
    });

    describe('translateRuleMessage', () => {
        it('translates ISO 5218 male and female guarantor balance rules', () => {
            const tSpy = vi.fn((key: string, options?: { defaultValue?: string }) => `${key}:${options?.defaultValue ?? ''}`);
            const t = tSpy as unknown as TFunction;

            expect(
                translateRuleMessage(
                    'According to ISO 5218, exactly one male (1) guarantor is required.',
                    t
                )
            ).toBe(
                'voucher.rule.genderBalanceMale:According to ISO 5218, exactly one male (1) guarantor is required.'
            );
            expect(
                translateRuleMessage(
                    'According to ISO 5218, exactly one female (2) guarantor is required.',
                    t
                )
            ).toBe(
                'voucher.rule.genderBalanceFemale:According to ISO 5218, exactly one female (2) guarantor is required.'
            );
        });

        it('returns unrecognized rule messages verbatim', () => {
            const tSpy = vi.fn();
            const t = tSpy as unknown as TFunction;

            expect(translateRuleMessage('Minimum 2 guarantors required', t)).toBe(
                'Minimum 2 guarantors required'
            );
            expect(tSpy).not.toHaveBeenCalled();
        });
    });

    describe('translateGentleHint', () => {
        const rawT = (key: string, options?: { defaultValue?: string; field?: string }) => {
            if (key === 'profile.field.coordinates') return 'Koordinaten';
            if (key === 'profile.field.url') return 'Webseite';
            if (key === 'profile.firstName') return 'Vorname';
            if (key === 'voucher.signRequest.openRuleCheck') return `Prüft: ${options?.field ?? ''}`;
            return options?.defaultValue ?? key;
        };
        const t = rawT as unknown as TFunction;

        it('translates known core gentle hints and maps aliases like location, website, and snake_case fields', () => {
            expect(translateGentleHint('Note: An open rule checks for your location', t)).toBe(
                'Prüft: Koordinaten'
            );
            expect(translateGentleHint('Note: An open rule checks for your website', t)).toBe(
                'Prüft: Webseite'
            );
            expect(translateGentleHint('Note: An open rule checks for your first_name', t)).toBe(
                'Prüft: Vorname'
            );
            expect(translateGentleHint('Note: An open rule checks for your custom_badge', t)).toBe(
                'Prüft: Custom Badge'
            );
        });

        it('returns non-matching gentle hint strings unchanged', () => {
            const tSpy = vi.fn();
            expect(translateGentleHint('Some other info message', tSpy as unknown as TFunction)).toBe(
                'Some other info message'
            );
            expect(tSpy).not.toHaveBeenCalled();
        });
    });
});
