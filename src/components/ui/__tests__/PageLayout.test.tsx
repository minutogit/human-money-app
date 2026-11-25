import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { PageLayout } from '../PageLayout';
import { PageHeader } from '../PageHeader';
import { BackButton } from '../BackButton';
import { PageOptionsPanel } from '../PageOptionsPanel';
import enJson from '../../../locales/en.json';

const en = enJson as Record<string, string>;

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

  it('renders optionsPanel when provided above children and inside maxWidth container', () => {
    render(
      <PageLayout
        title="Options Test"
        maxWidth="5xl"
        optionsPanel={<div data-testid="custom-options-panel">Options Content</div>}
      >
        <div data-testid="child-content">Main Children</div>
      </PageLayout>
    );

    const options = screen.getByTestId('custom-options-panel');
    expect(options).toBeInTheDocument();
    const children = screen.getByTestId('child-content');
    expect(children).toBeInTheDocument();

    // Verify ordering and common container
    const parentContainer = options.closest('.max-w-5xl');
    expect(parentContainer).toBeInTheDocument();
    expect(parentContainer).toContainElement(children);
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

describe('PageHeader', () => {
  it('returns null when title, onBack, and actions are omitted', () => {
    const { container } = render(<PageHeader />);
    expect(container.firstChild).toBeNull();
  });

  it('renders string title and description with canonical typography', () => {
    render(<PageHeader title="Test Title" description="Test Description" />);
    const heading = screen.getByRole('heading', { level: 1, name: 'Test Title' });
    expect(heading).toBeInTheDocument();
    expect(heading.className).toContain('text-xl sm:text-2xl font-black text-theme-primary');
    expect(screen.getByText('Test Description')).toBeInTheDocument();
  });

  it('renders custom ReactNode title and description', () => {
    render(
      <PageHeader
        title={<span data-testid="custom-title">Custom Node</span>}
        description={<div data-testid="custom-desc">Custom Description Node</div>}
      />
    );
    expect(screen.getByTestId('custom-title')).toBeInTheDocument();
    expect(screen.getByTestId('custom-desc')).toBeInTheDocument();
  });

  it('renders BackButton when onBack is passed and triggers onBack on click', () => {
    const handleBack = vi.fn();
    render(<PageHeader title="Back Test" onBack={handleBack} />);
    const backBtn = screen.getByRole('button', { name: 'Back' });
    expect(backBtn).toBeInTheDocument();
    fireEvent.click(backBtn);
    expect(handleBack).toHaveBeenCalledTimes(1);
  });

  it('renders actions inside flex container', () => {
    render(
      <PageHeader
        title="Actions Test"
        actions={<button type="button" data-testid="action-btn">Action</button>}
      />
    );
    expect(screen.getByTestId('action-btn')).toBeInTheDocument();
  });

  it('applies custom className prop', () => {
    const { container } = render(<PageHeader title="Custom Class" className="my-header-class" />);
    expect(container.querySelector('header')?.className).toContain('my-header-class');
  });
});

describe('PageOptionsPanel', () => {
  it('renders title, icon, close button, and children', () => {
    const handleClose = vi.fn();
    render(
      <PageOptionsPanel
        title="Panel Title"
        icon={<span data-testid="panel-icon">Icon</span>}
        onClose={handleClose}
      >
        <div data-testid="panel-body">Panel Body Content</div>
      </PageOptionsPanel>
    );

    expect(screen.getByText('Panel Title')).toBeInTheDocument();
    expect(screen.getByTestId('panel-icon')).toBeInTheDocument();
    expect(screen.getByTestId('panel-body')).toBeInTheDocument();

    const closeBtn = screen.getByRole('button', { name: /Schließen|Close/i });
    expect(closeBtn).toBeInTheDocument();
    fireEvent.click(closeBtn);
    expect(handleClose).toHaveBeenCalledTimes(1);
  });

  it('renders help topic icon when provided, opens HelpModal on click, and omits it when absent', () => {
    const { rerender } = render(
      <PageOptionsPanel
        title="Panel With Help"
        helpTopic="wotNetwork"
      >
        <div>Content</div>
      </PageOptionsPanel>
    );

    expect(screen.getByText('Panel With Help')).toBeInTheDocument();
    const helpBtn = screen.getByRole('button');
    fireEvent.click(helpBtn);
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByText(en['help.wotNetwork.title'])).toBeInTheDocument();

    rerender(
      <PageOptionsPanel title="Panel Without Help">
        <div>Content</div>
      </PageOptionsPanel>
    );
    expect(screen.queryByRole('button')).toBeNull();
  });
});
