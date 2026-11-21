import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { PrivacyToggle } from '../PrivacyToggle';
import { usePrivacyDetection } from '../../../hooks/usePrivacyDetection';
import { VoucherSummary, VoucherStandardDefinition } from '../../../types';

function DetectionHarness({
  selectedIds,
  vouchers,
  standards,
}: {
  selectedIds: string[];
  vouchers: VoucherSummary[];
  standards: Record<string, VoucherStandardDefinition>;
}) {
  const mode = usePrivacyDetection(selectedIds, vouchers, standards);
  return (
    <PrivacyToggle
      privacyMode={mode === 'stealth' || mode === 'public' ? mode : null}
      onPrivacyChange={vi.fn()}
      privacyRules={{
        mode,
        forced: mode === 'stealth' || mode === 'public',
      }}
      privacyError={false}
    />
  );
}

const makeVoucher = (id: string, standardUuid: string): VoucherSummary => ({
  localInstanceId: id,
  status: 'active',
  validUntil: '2026-12-31',
  creatorId: 'did:key:z123',
  description: 'Test',
  currentAmount: '10',
  unit: 'T',
  rawStandardName: 'Std',
  voucherStandardUuid: standardUuid,
  transactionCount: 0,
  signaturesCount: 0,
  hasCollateral: false,
  isTestVoucher: false,
  displayCurrency: 'T',
  displayStandardName: 'Std',
  allowPartialTransfers: true,
});

const makeStandard = (uuid: string, privacyMode: 'stealth' | 'public' | 'flexible'): VoucherStandardDefinition => ({
  immutable: {
    identity: { uuid, name: 'Std', abbreviation: 'T', issuerId: '0:is@did:key:z6Mkexample' },
    blueprint: { unit: 'T', primaryRedemptionType: 'goodsOrServices', collateralType: 'personalGuarantee' },
    features: {
      allowPartialTransfers: true,
      balancesAreSummable: true,
      amountDecimalPlaces: 2,
      privacyMode,
      allowedTTypes: ['transfer'],
    },
    issuance: {
      validityDurationRange: [],
      issuanceMinimumValidityDuration: 'P1M',
      additionalSignaturesRange: [0, 0],
      allowedSignatureRoles: [],
    },
    customRules: {},
  },
  mutable: {
    metadata: { issuerName: 'Issuer', keywords: [] },
    appConfig: {},
    i18n: {
      descriptions: {},
      footnotes: {},
      collateralDescriptions: {},
    },
  },
});

describe('PrivacyToggle & usePrivacyDetection', () => {
  it('displays forced Stealth badge instead of Public when standard requires stealth', () => {
    render(
      <DetectionHarness
        selectedIds={['v-stealth']}
        vouchers={[makeVoucher('v-stealth', 'std-stealth')]}
        standards={{ 'std-stealth': makeStandard('std-stealth', 'stealth') }}
      />
    );

    const badge = screen.getByTestId('privacy-forced-badge');
    expect(badge).toHaveTextContent(/Stealth/i);
    expect(badge.className).toContain('purple');
  });

  it('displays forced Public badge when standard requires public', () => {
    render(
      <DetectionHarness
        selectedIds={['v-public']}
        vouchers={[makeVoucher('v-public', 'std-public')]}
        standards={{ 'std-public': makeStandard('std-public', 'public') }}
      />
    );

    const badge = screen.getByTestId('privacy-forced-badge');
    expect(badge).toHaveTextContent(/Public/i);
    expect(badge.className).toContain('blue');
  });

  it('displays conflict warning when stealth and public standards are mixed', () => {
    render(
      <DetectionHarness
        selectedIds={['v-stealth', 'v-public']}
        vouchers={[
          makeVoucher('v-stealth', 'std-stealth'),
          makeVoucher('v-public', 'std-public'),
        ]}
        standards={{
          'std-stealth': makeStandard('std-stealth', 'stealth'),
          'std-public': makeStandard('std-public', 'public'),
        }}
      />
    );

    expect(screen.queryByTestId('privacy-forced-badge')).not.toBeInTheDocument();
    expect(screen.getAllByText(/Privacy Conflict/i).length).toBeGreaterThanOrEqual(1);
  });
});
