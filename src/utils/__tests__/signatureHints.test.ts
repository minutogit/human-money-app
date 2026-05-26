/* eslint-disable @typescript-eslint/no-explicit-any */
import { describe, it, expect } from 'vitest';
import { getMissingProfileHint, formatFieldName } from '../signatureHints';
import { VoucherStandardDefinition, PublicProfile } from '../../types';

describe('signatureHints', () => {
    describe('formatFieldName', () => {
        it('formats camelCase to Title Case', () => {
            expect(formatFieldName('someFieldName')).toBe('Some Field Name');
            expect(formatFieldName('needs')).toBe('Needs');
        });

        it('formats snake_case to Title Case', () => {
            expect(formatFieldName('some_field_name')).toBe('Some Field Name');
        });
    });

    describe('getMissingProfileHint', () => {
        const mockStandard = (customRules: Record<string, string>): VoucherStandardDefinition => ({
            immutable: {
                identity: {
                    uuid: 'test-uuid',
                    name: 'Test Standard',
                    abbreviation: 'TST'
                },
                blueprint: {} as any,
                features: {} as any,
                issuance: {
                    allowedSignatureRoles: []
                } as any,
                customRules: Object.fromEntries(
                    Object.entries(customRules).map(([name, expr]) => [
                        name,
                        { expression: expr, message: 'rule failed' }
                    ])
                )
            },
            mutable: {} as any
        });

        const mockProfile: PublicProfile = {
            id: 'did:key:123',
            firstName: 'Alice',
            lastName: 'Smith',
            gender: 'Female',
            email: 'alice@example.com',
            phone: '123456789'
        };

        const mockT = (key: string, options?: any) => {
            if (key === 'voucher.hint.prefix') return options?.defaultValue || 'This standard likely checks for: ';
            if (key === 'voucher.hint.connectorAnd') return options?.defaultValue || ' and ';
            if (key === 'voucher.hint.connectorComma') return options?.defaultValue || ', ';
            // Mock localized values for known keys
            if (key === 'profile.field.gender') return 'Geschlecht';
            if (key === 'profile.field.address') return 'Adresse';
            return options?.defaultValue || key;
        };

        it('returns null if profile is null', () => {
            const standard = mockStandard({ rule1: 's.details.gender == "Female"' });
            expect(getMissingProfileHint(standard, null, mockT as any)).toBeNull();
        });

        it('returns null if all required fields are present in profile', () => {
            const standard = mockStandard({ rule1: 's.details.gender == "Female"' });
            expect(getMissingProfileHint(standard, mockProfile, mockT as any)).toBeNull();
        });

        it('detects missing predefined field and localizes the label', () => {
            const standard = mockStandard({ rule1: 's.details.address != null' });
            const hint = getMissingProfileHint(standard, mockProfile, mockT as any);
            expect(hint).toBe('This standard likely checks for: Adresse');
        });

        it('detects multiple missing fields and formats them correctly', () => {
            const standard = mockStandard({
                rule1: 's.details.gender != null',
                rule2: 's.details.address != null',
                rule3: 's.details.coordinates != null'
            });
            const incompleteProfile: PublicProfile = {
                id: 'did:key:123',
                // gender, address, and coordinates missing
            };
            const hint = getMissingProfileHint(standard, incompleteProfile, mockT as any);
            // Gender -> translated via mockT to 'Geschlecht'
            // Address -> translated via mockT to 'Adresse'
            // Coordinates -> formatted to 'Coordinates' because no mockT override, fallback is defaultValue
            expect(hint).toBe('This standard likely checks for: Geschlecht, Adresse and Coordinates');
        });

        it('handles custom arbitrary fields not in the predefined map', () => {
            const standard = mockStandard({ rule1: 's.details.someCustomField == "xyz"' });
            const hint = getMissingProfileHint(standard, mockProfile, mockT as any);
            expect(hint).toBe('This standard likely checks for: Some Custom Field');
        });
    });
});
