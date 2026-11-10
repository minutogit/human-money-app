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

    const level2Card = screen.getByText(/contacts\.trustLevel2|Freund|Friend/i).closest('button')!;
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
});
