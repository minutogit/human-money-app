import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { AlertBanner } from '../AlertBanner';
import { SegmentedTabs, SegmentedTabItem } from '../SegmentedTabs';
import { ModalShell } from '../ModalShell';
import { HelpIcon } from '../HelpIcon';
import { PageLoadingState, PageEmptyState, PageErrorState } from '../PageState';

describe('AlertBanner', () => {
  it('renders all 4 style variants with correct color classes', () => {
    const { rerender } = render(
      <AlertBanner variant="success" data-testid="alert">Success Alert</AlertBanner>
    );
    let alert = screen.getByTestId('alert');
    expect(alert.className).toContain('bg-emerald-50 border-emerald-200 text-emerald-800');

    rerender(<AlertBanner variant="error" data-testid="alert">Error Alert</AlertBanner>);
    alert = screen.getByTestId('alert');
    expect(alert.className).toContain('bg-rose-50 border-rose-200 text-rose-800');

    rerender(<AlertBanner variant="warning" data-testid="alert">Warning Alert</AlertBanner>);
    alert = screen.getByTestId('alert');
    expect(alert.className).toContain('bg-amber-500/10 border-amber-500/25 text-amber-900');

    rerender(<AlertBanner variant="info" data-testid="alert">Info Alert</AlertBanner>);
    alert = screen.getByTestId('alert');
    expect(alert.className).toContain('bg-blue-50 border-blue-200 text-blue-800');
  });

  it('renders role="alert", optional title, message content, and custom icon', () => {
    render(
      <AlertBanner
        title="Wichtiger Hinweis"
        icon={<span data-testid="custom-alert-icon">⚡</span>}
      >
        Transaktion erfolgreich autorisiert.
      </AlertBanner>
    );

    const alert = screen.getByRole('alert');
    expect(alert).toBeInTheDocument();
    expect(screen.getByText('Wichtiger Hinweis')).toBeInTheDocument();
    expect(screen.getByText('Transaktion erfolgreich autorisiert.')).toBeInTheDocument();
    expect(screen.getByTestId('custom-alert-icon')).toBeInTheDocument();
  });

  it('renders action node and triggers onDismiss when close button is clicked', () => {
    const handleDismiss = vi.fn();
    render(
      <AlertBanner
        action={<button type="button">Details ansehen</button>}
        onDismiss={handleDismiss}
        dismissLabel="Schließen"
      >
        Aktualisierung verfügbar.
      </AlertBanner>
    );

    expect(screen.getByRole('button', { name: 'Details ansehen' })).toBeInTheDocument();
    const closeBtn = screen.getByRole('button', { name: 'Schließen' });
    expect(closeBtn).toBeInTheDocument();

    fireEvent.click(closeBtn);
    expect(handleDismiss).toHaveBeenCalledTimes(1);
  });
});

describe('SegmentedTabs', () => {
  const sampleTabs: SegmentedTabItem<string>[] = [
    { id: 'all', label: 'Alle', icon: <span data-testid="icon-all">*</span> },
    { id: 'received', label: 'Empfangen' },
    { id: 'own', label: 'Eigene', disabled: true },
  ];

  it('renders tablist and tabs with proper accessible roles and aria-selected states', () => {
    render(
      <SegmentedTabs
        tabs={sampleTabs}
        activeTab="received"
        onChange={vi.fn()}
      />
    );

    expect(screen.getByRole('tablist')).toBeInTheDocument();
    const tabs = screen.getAllByRole('tab');
    expect(tabs).toHaveLength(3);

    expect(tabs[0]).toHaveAttribute('aria-selected', 'false');
    expect(tabs[1]).toHaveAttribute('aria-selected', 'true');
    expect(tabs[2]).toHaveAttribute('aria-selected', 'false');
    expect(screen.getByTestId('icon-all')).toBeInTheDocument();
  });

  it('calls onChange with tab ID when enabled tab is clicked, but ignores disabled tabs', () => {
    const handleChange = vi.fn();
    render(
      <SegmentedTabs
        tabs={sampleTabs}
        activeTab="all"
        onChange={handleChange}
      />
    );

    // Click enabled tab
    fireEvent.click(screen.getByRole('tab', { name: 'Empfangen' }));
    expect(handleChange).toHaveBeenCalledWith('received');

    handleChange.mockClear();

    // Click disabled tab
    const disabledTab = screen.getByRole('tab', { name: 'Eigene' });
    expect(disabledTab).toBeDisabled();
    fireEvent.click(disabledTab);
    expect(handleChange).not.toHaveBeenCalled();
  });

  it('applies fullWidth flex styles when fullWidth is true', () => {
    const { rerender } = render(
      <SegmentedTabs
        tabs={sampleTabs}
        activeTab="all"
        onChange={vi.fn()}
        fullWidth
      />
    );

    const tablist = screen.getByRole('tablist');
    expect(tablist.className).toContain('flex flex-wrap sm:flex-nowrap w-full');

    rerender(
      <SegmentedTabs
        tabs={sampleTabs}
        activeTab="all"
        onChange={vi.fn()}
        fullWidth={false}
      />
    );
    expect(tablist.className).toContain('inline-flex flex-wrap justify-center max-w-full');
  });
});

describe('ModalShell', () => {
  it('renders nothing when isOpen is false', () => {
    const { container } = render(
      <ModalShell isOpen={false} title="Versteckt">
        <div>Geheimer Inhalt</div>
      </ModalShell>
    );

    expect(container.firstChild).toBeNull();
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('renders dialog with title, description, body, and footer when isOpen is true', () => {
    render(
      <ModalShell
        isOpen={true}
        title="Gutschein löschen"
        description="Möchtest du diesen Gutschein wirklich löschen?"
        footer={<button type="button">Bestätigen</button>}
      >
        <p>Aktion kann nicht rückgängig gemacht werden.</p>
      </ModalShell>
    );

    const dialog = screen.getByRole('dialog');
    expect(dialog).toBeInTheDocument();
    expect(dialog).toHaveAttribute('aria-modal', 'true');
    expect(screen.getByRole('heading', { level: 2, name: 'Gutschein löschen' })).toBeInTheDocument();
    expect(screen.getByText('Möchtest du diesen Gutschein wirklich löschen?')).toBeInTheDocument();
    expect(screen.getByText('Aktion kann nicht rückgängig gemacht werden.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Bestätigen' })).toBeInTheDocument();
  });

  it('closes on Escape key press by default when onClose is provided', () => {
    const handleClose = vi.fn();
    render(
      <ModalShell isOpen={true} onClose={handleClose} title="Escape Test">
        <div>Body</div>
      </ModalShell>
    );

    fireEvent.keyDown(window, { key: 'Escape' });
    expect(handleClose).toHaveBeenCalledTimes(1);
  });

  it('does NOT close on Escape key when closeOnEscape is false', () => {
    const handleClose = vi.fn();
    render(
      <ModalShell
        isOpen={true}
        onClose={handleClose}
        closeOnEscape={false}
        title="No Escape"
      >
        <div>Body</div>
      </ModalShell>
    );

    fireEvent.keyDown(window, { key: 'Escape' });
    expect(handleClose).not.toHaveBeenCalled();
  });

  it('respects closeOnBackdrop setting', () => {
    const handleClose = vi.fn();
    const { rerender } = render(
      <ModalShell
        isOpen={true}
        onClose={handleClose}
        closeOnBackdrop={false}
        title="Backdrop Test"
      >
        <div data-testid="dialog-inner">Dialog Content</div>
      </ModalShell>
    );

    const backdrop = screen.getByRole('dialog').parentElement as HTMLDivElement;
    expect(backdrop).not.toBeNull();

    // Clicking backdrop when closeOnBackdrop=false should NOT close
    fireEvent.click(backdrop);
    expect(handleClose).not.toHaveBeenCalled();

    // Enable closeOnBackdrop
    rerender(
      <ModalShell
        isOpen={true}
        onClose={handleClose}
        closeOnBackdrop={true}
        title="Backdrop Test"
      >
        <div data-testid="dialog-inner">Dialog Content</div>
      </ModalShell>
    );

    // Clicking inside the dialog content should NOT trigger close
    const innerContent = screen.getByTestId('dialog-inner');
    fireEvent.click(innerContent);
    expect(handleClose).not.toHaveBeenCalled();

    // Clicking the outer backdrop itself SHOULD trigger close
    const updatedBackdrop = screen.getByRole('dialog').parentElement as HTMLDivElement;
    fireEvent.click(updatedBackdrop);
    expect(handleClose).toHaveBeenCalledTimes(1);
  });

  it('applies maxWidth classes properly', () => {
    render(
      <ModalShell isOpen={true} maxWidth="2xl" title="Großes Modal">
        <div>Content</div>
      </ModalShell>
    );

    const dialog = screen.getByRole('dialog');
    expect(dialog.className).toContain('max-w-2xl');
  });

  it('portals its dialog into document.body outside an overflow-hidden wrapper', () => {
    render(
      <div className="overflow-hidden max-h-10 transform scale-50" data-testid="trapping-parent">
        <ModalShell isOpen={true} title="Portaled Modal">
          <p>Modal inside trapping ancestor</p>
        </ModalShell>
      </div>
    );

    const trappingParent = screen.getByTestId('trapping-parent');
    expect(trappingParent.children.length).toBe(0);

    const dialog = screen.getByRole('dialog');
    expect(dialog).toBeInTheDocument();
    expect(trappingParent.contains(dialog)).toBe(false);
    expect(document.body.contains(dialog)).toBe(true);
  });

  it('supports custom zIndexClass with default z-[100]', () => {
    const { rerender } = render(
      <ModalShell isOpen={true} title="Default Z-Index">
        <div>Content</div>
      </ModalShell>
    );

    let backdrop = screen.getByRole('dialog').parentElement as HTMLDivElement;
    expect(backdrop.className).toContain('z-[100]');

    rerender(
      <ModalShell isOpen={true} zIndexClass="z-[150]" title="Custom Z-Index">
        <div>Content</div>
      </ModalShell>
    );

    backdrop = screen.getByRole('dialog').parentElement as HTMLDivElement;
    expect(backdrop.className).toContain('z-[150]');
  });
});

describe('PageState', () => {
  describe('PageLoadingState', () => {
    it('renders loading message and spinner', () => {
      render(<PageLoadingState message="Lade Transaktionen..." />);
      expect(screen.getByText('Lade Transaktionen...')).toBeInTheDocument();
    });

    it('falls back to default loading text when message is omitted', () => {
      render(<PageLoadingState />);
      // Fallback from en.json / common.loading
      expect(screen.getByText(/loading/i)).toBeInTheDocument();
    });
  });

  describe('PageEmptyState', () => {
    it('renders title, description, icon, and action button', () => {
      const handleAction = vi.fn();
      render(
        <PageEmptyState
          icon={<span data-testid="empty-icon">📭</span>}
          title="Keine Gutscheine vorhanden"
          description="Erstelle deinen ersten Gutschein, um loszulegen."
          action={<button type="button" onClick={handleAction}>Neuer Gutschein</button>}
        />
      );

      expect(screen.getByTestId('empty-icon')).toBeInTheDocument();
      expect(screen.getByRole('heading', { level: 3, name: 'Keine Gutscheine vorhanden' })).toBeInTheDocument();
      expect(screen.getByText('Erstelle deinen ersten Gutschein, um loszulegen.')).toBeInTheDocument();

      const actionBtn = screen.getByRole('button', { name: 'Neuer Gutschein' });
      expect(actionBtn).toBeInTheDocument();
      fireEvent.click(actionBtn);
      expect(handleAction).toHaveBeenCalledTimes(1);
    });

    it('renders custom ReactNode title', () => {
      render(
        <PageEmptyState
          title={<span data-testid="node-title">Spezial-Titel</span>}
        />
      );
      expect(screen.getByTestId('node-title')).toBeInTheDocument();
    });
  });

  describe('PageErrorState', () => {
    it('renders error message and default error title', () => {
      render(<PageErrorState message="Verbindung zum Shard fehlgeschlagen." />);
      const alert = screen.getByRole('alert');
      expect(alert).toBeInTheDocument();
      expect(screen.getByText('Verbindung zum Shard fehlgeschlagen.')).toBeInTheDocument();
    });

    it('renders retry button and triggers onRetry callback when clicked', () => {
      const handleRetry = vi.fn();
      render(
        <PageErrorState
          title="Netzwerkfehler"
          message="Bitte erneut versuchen."
          onRetry={handleRetry}
        />
      );

      expect(screen.getByText('Netzwerkfehler')).toBeInTheDocument();
      expect(screen.getByText('Bitte erneut versuchen.')).toBeInTheDocument();

      const retryBtn = screen.getByRole('button', { name: /retry/i });
      expect(retryBtn).toBeInTheDocument();

      fireEvent.click(retryBtn);
      expect(handleRetry).toHaveBeenCalledTimes(1);
    });

    it('renders custom action alongside or instead of retry button', () => {
      render(
        <PageErrorState
          message="Fehler aufgetreten."
          action={<button type="button">Hilfe öffnen</button>}
        />
      );

      expect(screen.getByRole('button', { name: 'Hilfe öffnen' })).toBeInTheDocument();
    });
  });
});

describe('HelpIcon and HelpModal', () => {
  it('opens HelpModal in document.body on click and closes on Close button', () => {
    render(
      <div className="overflow-hidden" data-testid="trapping-wrapper">
        <HelpIcon topic="posMode" />
      </div>
    );

    // Initial state: HelpModal is not open
    expect(screen.queryByRole('dialog')).toBeNull();

    // Click the help button
    const helpBtn = screen.getByRole('button');
    fireEvent.click(helpBtn);

    // HelpModal should now be open portaled into document.body
    const dialog = screen.getByRole('dialog');
    expect(dialog).toBeInTheDocument();
    const trappingWrapper = screen.getByTestId('trapping-wrapper');
    expect(trappingWrapper.contains(dialog)).toBe(false);
    expect(document.body.contains(dialog)).toBe(true);

    // Close on Close button
    const closeBtn = screen.getByRole('button', { name: /close|schließen/i });
    fireEvent.click(closeBtn);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('closes HelpModal on Escape key', () => {
    render(<HelpIcon topic="settingsPrivacy" />);

    // Click to open
    fireEvent.click(screen.getByRole('button'));
    expect(screen.getByRole('dialog')).toBeInTheDocument();

    // Press Escape
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('closes HelpModal on backdrop click but not on dialog click', () => {
    render(<HelpIcon topic="exclusiveGateways" />);

    // Click to open
    fireEvent.click(screen.getByRole('button'));
    const dialog = screen.getByRole('dialog');
    const backdrop = dialog.parentElement as HTMLDivElement;

    // Click inside dialog content should not close
    fireEvent.click(dialog);
    expect(screen.getByRole('dialog')).toBeInTheDocument();

    // Click backdrop should close
    fireEvent.click(backdrop);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('renders HelpModal at z-[110] so it stacks cleanly above ModalShell at z-[100]', () => {
    render(
      <ModalShell isOpen={true} title="Parent Modal">
        <HelpIcon topic="deviceBinding" />
      </ModalShell>
    );

    // Click help button inside ModalShell
    const helpBtn = screen.getByRole('button');
    fireEvent.click(helpBtn);

    const dialogs = screen.getAllByRole('dialog');
    expect(dialogs).toHaveLength(2);

    const modalShellBackdrop = dialogs[0].parentElement as HTMLDivElement;
    const helpModalBackdrop = dialogs[1].parentElement as HTMLDivElement;

    expect(modalShellBackdrop.className).toContain('z-[100]');
    expect(helpModalBackdrop.className).toContain('z-[110]');
  });
});

