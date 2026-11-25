// src/components/__tests__/TrustLevelRadioCards.test.tsx
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { TrustLevelRadioCards } from '../TrustLevelRadioCards';

describe('TrustLevelRadioCards Component', () => {
  it('renders all trust levels and unrated option by default', () => {
    const onChange = vi.fn();
    render(<TrustLevelRadioCards value={null} onChange={onChange} allowUnrated={true} />);

    expect(screen.getByText(/contacts\.trustLevelUnrated|Noch nicht eingestuft|Not rated yet/i)).toBeInTheDocument();
    expect(screen.getByText(/contacts\.trustLevel0|Als Mensch bestätigt|Verified as human/i)).toBeInTheDocument();
    expect(screen.getByText(/contacts\.trustLevel1|Bekannt|Acquaintance/i)).toBeInTheDocument();
    expect(screen.getByText(/contacts\.trustLevel2|Freund|Friend/i)).toBeInTheDocument();
    expect(screen.getByText(/contacts\.trustLevel3|Enger Vertrauter|Close confidant/i)).toBeInTheDocument();
  });

  it('calls onChange with selected level when clicking a radio card', () => {
    const onChange = vi.fn();
    render(<TrustLevelRadioCards value={1} onChange={onChange} allowUnrated={true} />);

    const level2Card = screen.getByTestId('trust-level-2');
    fireEvent.click(level2Card);

    expect(onChange).toHaveBeenCalledWith(2);
  });

  it('displays additional hint for level 0 when selected', () => {
    const onChange = vi.fn();
    const { rerender } = render(<TrustLevelRadioCards value={null} onChange={onChange} allowUnrated={true} />);

    expect(screen.queryByTestId('trust-hint-level-0')).not.toBeInTheDocument();

    rerender(<TrustLevelRadioCards value={0} onChange={onChange} allowUnrated={true} />);

    expect(screen.getByTestId('trust-hint-level-0')).toBeInTheDocument();
    expect(screen.getByText(/Deine Kontakte sehen, dass du diese Person als echten Menschen bestätigst|Your contacts see that you verify this person as a real human/i)).toBeInTheDocument();
  });

  it('displays mutual responsibility hint for levels 1 to 3 when selected, without bürgen or haften', () => {
    const onChange = vi.fn();
    const { rerender } = render(<TrustLevelRadioCards value={2} onChange={onChange} allowUnrated={true} />);

    const hintContainer = screen.getByTestId('trust-hint-levels-1-to-3');
    expect(hintContainer).toBeInTheDocument();
    expect(hintContainer.textContent).toMatch(/Du stehst freiwillig ein Stück weit für diese Person ein|You voluntarily stand up for this person/i);

    // Strict rule: no bürgen or haften!
    expect(hintContainer.textContent?.toLowerCase()).not.toContain('bürgen');
    expect(hintContainer.textContent?.toLowerCase()).not.toContain('haften');

    // Also holds for level 1
    rerender(<TrustLevelRadioCards value={1} onChange={onChange} allowUnrated={true} />);
    expect(screen.getByTestId('trust-hint-levels-1-to-3')).toBeInTheDocument();

    // Also holds for level 3
    rerender(<TrustLevelRadioCards value={3} onChange={onChange} allowUnrated={true} />);
    expect(screen.getByTestId('trust-hint-levels-1-to-3')).toBeInTheDocument();

    // Verifies privacy disclosure note is shown for level >= 1
    const privacyNote = screen.getByTestId('trust-privacy-disclosure-note');
    expect(privacyNote).toBeInTheDocument();
    expect(privacyNote.textContent).toMatch(/contacts\.trustPrivacyDisclaimer|kryptografisch geschützt|shared cryptographically/i);
  });

  it('hides unrated option when allowUnrated is false', () => {
    const onChange = vi.fn();
    render(<TrustLevelRadioCards value={1} onChange={onChange} allowUnrated={false} />);

    expect(screen.queryByText(/contacts\.trustLevelUnrated|Noch nicht eingestuft|Not rated yet/i)).not.toBeInTheDocument();
  });

  it('restricts options when allowedLevels is provided', () => {
    const onChange = vi.fn();
    render(<TrustLevelRadioCards value={0} onChange={onChange} allowUnrated={false} allowedLevels={[0, 1]} />);

    expect(screen.getByText(/contacts\.trustLevel0|Als Mensch bestätigt|Verified as human/i)).toBeInTheDocument();
    expect(screen.getByText(/contacts\.trustLevel1|Bekannt|Acquaintance/i)).toBeInTheDocument();
    expect(screen.queryByText(/contacts\.trustLevel2|Freund|Friend/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/contacts\.trustLevel3|Enger Vertrauter|Close confidant/i)).not.toBeInTheDocument();
  });

  it('exposes radiogroup semantics, exact aria-checked states, custom idPrefix, and unrated selection callback', () => {
    const onChange = vi.fn();
    render(
      <TrustLevelRadioCards
        value={2}
        onChange={onChange}
        allowUnrated={true}
        idPrefix="custom-trust"
      />
    );

    const group = screen.getByRole('radiogroup');
    expect(group).toBeInTheDocument();

    const cardUnrated = screen.getByTestId('trust-level-unrated');
    const card0 = screen.getByTestId('trust-level-0');
    const card2 = screen.getByTestId('trust-level-2');

    expect(card2).toHaveAttribute('id', 'custom-trust-2');
    expect(card2).toHaveAttribute('aria-checked', 'true');
    expect(card0).toHaveAttribute('aria-checked', 'false');
    expect(cardUnrated).toHaveAttribute('aria-checked', 'false');

    fireEvent.click(cardUnrated);
    expect(onChange).toHaveBeenCalledWith(null);
  });

  it('keeps unrated option visible alongside restricted allowedLevels when allowUnrated is true', () => {
    const onChange = vi.fn();
    render(<TrustLevelRadioCards value={null} onChange={onChange} allowUnrated={true} allowedLevels={[0, 1]} />);

    expect(screen.getByTestId('trust-level-unrated')).toBeInTheDocument();
    expect(screen.getByTestId('trust-level-0')).toBeInTheDocument();
    expect(screen.getByTestId('trust-level-1')).toBeInTheDocument();
    expect(screen.queryByTestId('trust-level-2')).not.toBeInTheDocument();
    expect(screen.queryByTestId('trust-level-3')).not.toBeInTheDocument();
  });
});
