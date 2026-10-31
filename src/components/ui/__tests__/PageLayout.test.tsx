import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { PageLayout } from '../PageLayout';
import { BackButton } from '../BackButton';

describe('PageLayout', () => {
  it('renders title as string, description, and children', () => {
    render(
      <PageLayout
        title="Adressbuch"
        description="Verwalte deine Kontakte und Vertrauensstellungen"
      >
        <div data-testid="page-content">Kontakte-Liste</div>
      </PageLayout>
    );

    const heading = screen.getByRole('heading', { level: 1, name: 'Adressbuch' });
    expect(heading).toBeInTheDocument();
    expect(screen.getByText('Verwalte deine Kontakte und Vertrauensstellungen')).toBeInTheDocument();
    expect(screen.getByTestId('page-content')).toHaveTextContent('Kontakte-Liste');
  });

  it('renders title as a custom ReactNode', () => {
    render(
      <PageLayout
        title={<span data-testid="custom-title-node">Custom Node Title</span>}
      >
        <div>Content</div>
      </PageLayout>
    );

    expect(screen.getByTestId('custom-title-node')).toBeInTheDocument();
  });

  it('renders customHeader when provided, bypassing default header markup', () => {
    render(
      <PageLayout
        customHeader={<div data-testid="custom-header-bar">Custom Header Element</div>}
        title="Ignored Default Title"
      >
        <div>Content</div>
      </PageLayout>
    );

    expect(screen.getByTestId('custom-header-bar')).toBeInTheDocument();
    expect(screen.queryByText('Ignored Default Title')).toBeNull();
  });

  it('renders no header element when title, onBack, actions, and customHeader are omitted', () => {
    const { container } = render(
      <PageLayout>
        <div data-testid="bare-content">No Header Content</div>
      </PageLayout>
    );

    expect(container.querySelector('header')).toBeNull();
    expect(screen.getByTestId('bare-content')).toBeInTheDocument();
  });

  it('renders BackButton when onBack is passed and triggers onBack when clicked', () => {
    const handleBack = vi.fn();
    render(
      <PageLayout title="Details" onBack={handleBack}>
        <div>Details Content</div>
      </PageLayout>
    );

    const backBtn = screen.getByRole('button', { name: 'Back' });
    expect(backBtn).toBeInTheDocument();
    expect(backBtn).toHaveAttribute('title', 'Back');
    expect(backBtn).toHaveAttribute('aria-label', 'Back');

    fireEvent.click(backBtn);
    expect(handleBack).toHaveBeenCalledTimes(1);
  });

  it('renders optional maxWidth container when specified', () => {
    const { rerender } = render(
      <PageLayout title="Width 4xl" maxWidth="4xl">
        <div data-testid="boxed-child">Boxed 4xl Content</div>
      </PageLayout>
    );

    let boxedWrapper = screen.getByTestId('boxed-child').parentElement;
    expect(boxedWrapper?.className).toContain('max-w-4xl mx-auto w-full');

    rerender(
      <PageLayout title="Width 6xl" maxWidth="6xl">
        <div data-testid="boxed-child">Boxed 6xl Content</div>
      </PageLayout>
    );
    boxedWrapper = screen.getByTestId('boxed-child').parentElement;
    expect(boxedWrapper?.className).toContain('max-w-6xl mx-auto w-full');

    rerender(
      <PageLayout title="No MaxWidth">
        <div data-testid="unboxed-child">Unboxed Content</div>
      </PageLayout>
    );
    const unboxedParent = screen.getByTestId('unboxed-child').parentElement;
    expect(unboxedParent?.className).not.toContain('max-w-');
    expect(unboxedParent?.className).toContain('flex-grow py-2');
  });

  it('applies custom className and contentClassName props', () => {
    const { container } = render(
      <PageLayout
        title="Custom Classes"
        className="extra-root-class"
        contentClassName="extra-content-class"
      >
        <div>Content</div>
      </PageLayout>
    );

    const root = container.firstElementChild;
    expect(root?.className).toContain('extra-root-class');
    const contentArea = container.querySelector('.extra-content-class');
    expect(contentArea).toBeInTheDocument();
  });

  it('ensures the title wrapper carries the anti-squish class and actions render inside flex-wrap container', () => {
    render(
      <PageLayout
        title="Adressbuch"
        description="Kontakte"
        actions={
          <div data-testid="action-buttons">
            <button type="button">Filter</button>
            <button type="button">Add Contact</button>
          </div>
        }
      >
        <div>Body Content</div>
      </PageLayout>
    );

    const heading = screen.getByRole('heading', { level: 1, name: 'Adressbuch' });
    const titleColumn = heading.closest('.min-w-\\[min\\(100\\%\\,14rem\\)\\]');
    expect(titleColumn).not.toBeNull();
    expect(titleColumn?.className).toContain('flex-[1_1_14rem]');

    const headerRow = titleColumn?.parentElement;
    expect(headerRow?.className).toContain('flex flex-wrap items-center justify-between gap-x-4 gap-y-3');

    const actionsContainer = screen.getByTestId('action-buttons').parentElement;
    expect(actionsContainer?.className).toContain('flex flex-wrap items-center gap-2 sm:justify-end shrink-0 max-w-full ml-auto');
  });
});

describe('BackButton', () => {
  it('renders standard icon back button with accessible title, aria-label, and click handler', () => {
    const handleClick = vi.fn();
    render(<BackButton onClick={handleClick} />);

    const button = screen.getByRole('button', { name: 'Back' });
    expect(button).toBeInTheDocument();
    expect(button).toHaveAttribute('aria-label', 'Back');
    expect(button).toHaveAttribute('title', 'Back');
    expect(button.className).toContain('rounded-full');

    fireEvent.click(button);
    expect(handleClick).toHaveBeenCalledTimes(1);
  });

  it('renders back button with text label when label prop is provided', () => {
    const handleClick = vi.fn();
    render(<BackButton onClick={handleClick} label="Zur Übersicht" />);

    expect(screen.getByText('Zur Übersicht')).toBeInTheDocument();
    const button = screen.getByRole('button');
    expect(button).toHaveAttribute('aria-label', 'Back');
    expect(button.className).toContain('rounded-2xl');

    fireEvent.click(button);
    expect(handleClick).toHaveBeenCalledTimes(1);
  });

  it('respects custom title prop overriding the localized default', () => {
    render(<BackButton onClick={vi.fn()} title="Zurück zum Wallet" />);

    const button = screen.getByRole('button', { name: 'Zurück zum Wallet' });
    expect(button).toHaveAttribute('title', 'Zurück zum Wallet');
    expect(button).toHaveAttribute('aria-label', 'Zurück zum Wallet');
  });

  it('merges custom className prop', () => {
    render(<BackButton onClick={vi.fn()} className="custom-back-btn" />);

    const button = screen.getByRole('button');
    expect(button.className).toContain('custom-back-btn');
  });
});
