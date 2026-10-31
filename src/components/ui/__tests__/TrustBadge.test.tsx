import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { TrustBadge } from '../TrustBadge';
import { VoucherTrustDto } from '../../../types';

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown>) => {
      if (options?.percent !== undefined && options?.name !== undefined) {
        return `${key}:${String(options.percent)}%:${String(options.name)}`;
      }
      if (options?.percent !== undefined) {
        return `${key}:${String(options.percent)}%`;
      }
      return key;
    },
    i18n: { language: 'en' },
  }),
}));

describe('TrustBadge', () => {
  it('returns null if no trust object is provided', () => {
    const { container } = render(<TrustBadge trust={null} />);
    expect(container.firstChild).toBeNull();
  });

  it('renders own voucher state with blue styling', () => {
    const trust: VoucherTrustDto = {
      score: 1.0,
      trustPercent: 100,
      isOwnVoucher: true,
      blockedByOffender: false,
      shortestHops: 0,
      additionalPathsCount: 0,
      viaGuarantor: false,
      bestPathNames: [],
    };
    render(<TrustBadge trust={trust} />);
    const badge = screen.getByTestId('trust-badge');
    expect(badge).toHaveTextContent('voucher.trustOwn');
    expect(badge.className).toContain('sky');
  });

  it('renders high trust >= 60% via contact/guarantor with emerald green, contact name, and additional paths', () => {
    const trust: VoucherTrustDto = {
      score: 0.85,
      trustPercent: 85,
      isOwnVoucher: false,
      blockedByOffender: false,
      shortestHops: 2,
      primaryBridgeName: 'Alice Smith',
      additionalPathsCount: 1,
      viaGuarantor: true,
      bestPathNames: ['Alice Smith'],
    };
    render(<TrustBadge trust={trust} />);
    const badge = screen.getByTestId('trust-badge');
    expect(badge).toHaveTextContent('voucher.trustViaGuarantor:85%:Alice Smith (+1)');
    expect(badge.className).toContain('emerald');
  });

  it('renders medium trust 25%-59% with amber styling', () => {
    const trust: VoucherTrustDto = {
      score: 0.45,
      trustPercent: 45,
      isOwnVoucher: false,
      blockedByOffender: false,
      shortestHops: 2,
      primaryBridgeName: 'Bob',
      additionalPathsCount: 0,
      viaGuarantor: false,
      bestPathNames: ['Bob'],
    };
    render(<TrustBadge trust={trust} />);
    const badge = screen.getByTestId('trust-badge');
    expect(badge).toHaveTextContent('voucher.trustViaContact:45%:Bob');
    expect(badge.className).toContain('amber');
  });

  it('renders unknown 0% with neutral slate styling (NOT red)', () => {
    const trust: VoucherTrustDto = {
      score: 0,
      trustPercent: 0,
      isOwnVoucher: false,
      blockedByOffender: false,
      shortestHops: null,
      additionalPathsCount: 0,
      viaGuarantor: false,
      bestPathNames: [],
    };
    render(<TrustBadge trust={trust} />);
    const badge = screen.getByTestId('trust-badge');
    expect(badge).toHaveTextContent('voucher.trustUnknown');
    expect(badge.className).toContain('slate');
    expect(badge.className).not.toContain('rose');
    expect(badge.className).not.toContain('red');
  });

  it('renders blocked by offender with red warning styling', () => {
    const trust: VoucherTrustDto = {
      score: 0,
      trustPercent: 0,
      isOwnVoucher: false,
      blockedByOffender: true,
      shortestHops: null,
      additionalPathsCount: 0,
      viaGuarantor: false,
      bestPathNames: [],
    };
    render(<TrustBadge trust={trust} />);
    const badge = screen.getByTestId('trust-badge');
    expect(badge).toHaveTextContent('voucher.trustOffender');
    expect(badge.className).toContain('rose');
  });

  it('renders direct trust when shortestHops is 1 and not via guarantor', () => {
    const trust: VoucherTrustDto = {
      score: 0.85,
      trustPercent: 85,
      isOwnVoucher: false,
      blockedByOffender: false,
      shortestHops: 1,
      additionalPathsCount: 0,
      viaGuarantor: false,
      bestPathNames: [],
    };
    render(<TrustBadge trust={trust} />);
    const badge = screen.getByTestId('trust-badge');
    expect(badge).toHaveTextContent('voucher.trustDirect:85%');
  });

  it('renders network trust when no primary bridge is set', () => {
    const trust: VoucherTrustDto = {
      score: 0.3,
      trustPercent: 30,
      isOwnVoucher: false,
      blockedByOffender: false,
      shortestHops: 3,
      primaryBridgeName: null,
      additionalPathsCount: 2,
      viaGuarantor: false,
      bestPathNames: [],
    };
    render(<TrustBadge trust={trust} />);
    const badge = screen.getByTestId('trust-badge');
    expect(badge).toHaveTextContent('voucher.trustViaNetwork:30% (+2)');
  });
});
