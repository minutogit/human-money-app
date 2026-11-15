import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { UiModeProvider, UI_MODE_STORAGE_KEY, useUiMode } from '../../../context/UiModeContext';
import { Advanced } from '../Advanced';
import { AdvancedDisclosure } from '../AdvancedDisclosure';
import { TechId } from '../TechId';
import { PresetOrCustom } from '../PresetOrCustom';
import { UiModeToggle } from '../UiModeToggle';
import { PageHeader } from '../PageHeader';

vi.mock('../../../utils/clipboard', () => ({
    copyToClipboard: vi.fn().mockResolvedValue(true),
}));

import { copyToClipboard } from '../../../utils/clipboard';

function ModeReader() {
    const { mode, isExpert } = useUiMode();
    return <span data-testid="mode-reader">{`${mode}:${isExpert}`}</span>;
}

describe('UI Mode Primitives', () => {
    beforeEach(() => {
        localStorage.clear();
        vi.clearAllMocks();
    });

    it('defaults UiModeProvider to simple mode and persists toggles across UiModeToggle variants', () => {
        render(
            <UiModeProvider>
                <ModeReader />
                <UiModeToggle variant="compact" />
                <UiModeToggle variant="full" />
            </UiModeProvider>
        );

        expect(screen.getByTestId('mode-reader')).toHaveTextContent('simple:false');
        expect(screen.getByTestId('ui-mode-simple')).toHaveAttribute('aria-pressed', 'true');
        expect(screen.getByTestId('ui-mode-expert')).toHaveAttribute('aria-pressed', 'false');

        fireEvent.click(screen.getByTestId('ui-mode-card-expert'));

        expect(screen.getByTestId('mode-reader')).toHaveTextContent('expert:true');
        expect(localStorage.getItem(UI_MODE_STORAGE_KEY)).toBe('expert');
        expect(screen.getByTestId('ui-mode-expert')).toHaveAttribute('aria-pressed', 'true');

        fireEvent.click(screen.getByTestId('ui-mode-simple'));
        expect(screen.getByTestId('mode-reader')).toHaveTextContent('simple:false');
        expect(localStorage.getItem(UI_MODE_STORAGE_KEY)).toBe('simple');
    });

    it('renders <Advanced> children only in expert mode', () => {
        render(
            <UiModeProvider>
                <UiModeToggle variant="compact" />
                <Advanced>
                    <div data-testid="expert-only-block">Technical JSON</div>
                </Advanced>
            </UiModeProvider>
        );

        expect(screen.queryByTestId('expert-only-block')).not.toBeInTheDocument();

        fireEvent.click(screen.getByTestId('ui-mode-expert'));
        expect(screen.getByTestId('expert-only-block')).toBeInTheDocument();
    });

    it('keeps AdvancedDisclosure collapsed in simple mode (with customized badge), opens in expert mode, and auto-opens + focuses on error', () => {
        const { rerender } = render(
            <UiModeProvider>
                <UiModeToggle variant="compact" />
                <AdvancedDisclosure
                    title="Network Server"
                    isCustom={true}
                    hasError={false}
                    toggleTestId="adv-disc-toggle"
                    contentTestId="adv-disc-content"
                >
                    <input data-testid="adv-disc-input" defaultValue="" />
                </AdvancedDisclosure>
            </UiModeProvider>
        );

        // Simple mode: collapsed, shows customized badge
        expect(screen.queryByTestId('adv-disc-content')).not.toBeInTheDocument();
        expect(screen.getByTestId('advanced-disclosure-custom-badge')).toBeInTheDocument();

        // Expert mode: automatically opens
        fireEvent.click(screen.getByTestId('ui-mode-expert'));
        expect(screen.getByTestId('adv-disc-content')).toBeInTheDocument();
        expect(screen.queryByTestId('advanced-disclosure-custom-badge')).not.toBeInTheDocument();

        // Back to simple mode: collapsed again
        fireEvent.click(screen.getByTestId('ui-mode-simple'));
        expect(screen.queryByTestId('adv-disc-content')).not.toBeInTheDocument();

        // Trigger validation error: opens and focuses input
        rerender(
            <UiModeProvider>
                <UiModeToggle variant="compact" />
                <AdvancedDisclosure
                    title="Network Server"
                    isCustom={true}
                    hasError={true}
                    toggleTestId="adv-disc-toggle"
                    contentTestId="adv-disc-content"
                >
                    <input data-testid="adv-disc-input" defaultValue="" />
                </AdvancedDisclosure>
            </UiModeProvider>
        );

        expect(screen.getByTestId('adv-disc-content')).toBeInTheDocument();
        expect(screen.getByTestId('adv-disc-input')).toHaveFocus();
    });

    it('renders TechId without raw did:key in DOM in simple mode, copies full value, and shows full ID in expert mode', () => {
        const sampleDid = 'did:key:z6MkuSampleUserIdentifierALICE1';

        const { container } = render(
            <UiModeProvider>
                <UiModeToggle variant="compact" />
                <TechId value={sampleDid} kind="did" />
            </UiModeProvider>
        );

        // Simple mode: shows short fingerprint, never leaks raw did:key string into DOM attributes or text
        expect(screen.getByTestId('tech-id-value')).toHaveTextContent('…ALICE1');
        expect(container.innerHTML).not.toContain('did:key:');

        fireEvent.click(screen.getByTestId('tech-id-copy'));
        expect(copyToClipboard).toHaveBeenCalledWith(sampleDid);

        // Expert mode: reveals full ID
        fireEvent.click(screen.getByTestId('ui-mode-expert'));
        expect(screen.getByTestId('tech-id-value')).toHaveTextContent(sampleDid);
    });

    it('shows presets + custom badge in simple mode and custom controls in expert mode in PresetOrCustom', () => {
        render(
            <UiModeProvider>
                <UiModeToggle variant="compact" />
                <PresetOrCustom<'near' | 'medium' | 'far'>
                    presets={[
                        { id: 'near', title: 'Nah', description: 'Direct only' },
                        { id: 'medium', title: 'Mittel', description: 'Standard', recommended: true },
                        { id: 'far', title: 'Weit', description: 'Extended' },
                    ]}
                    value="custom"
                    onChange={() => {}}
                    renderCustom={() => <div data-testid="custom-sliders">Raw Hops Slider</div>}
                />
            </UiModeProvider>
        );

        expect(screen.getByTestId('preset-custom-status')).toBeInTheDocument();
        expect(screen.queryByTestId('custom-sliders')).not.toBeInTheDocument();

        fireEvent.click(screen.getByTestId('ui-mode-expert'));
        expect(screen.getByTestId('custom-sliders')).toBeInTheDocument();
    });

    it('bundles PageHeader.advancedActions into a more menu in simple mode and renders inline in expert mode', () => {
        render(
            <UiModeProvider>
                <UiModeToggle variant="compact" />
                <PageHeader
                    title="My Page"
                    actions={<button type="button">Primary</button>}
                    advancedActions={<button type="button" data-testid="adv-header-action">JSON Export</button>}
                />
            </UiModeProvider>
        );

        // Simple mode: action is inside menu, hidden until opened
        expect(screen.queryByTestId('adv-header-action')).not.toBeInTheDocument();
        fireEvent.click(screen.getByTestId('page-header-more-actions'));
        expect(screen.getByTestId('page-header-more-menu')).toBeInTheDocument();
        expect(screen.getByTestId('adv-header-action')).toBeInTheDocument();

        // Escape closes menu
        fireEvent.keyDown(document, { key: 'Escape' });
        expect(screen.queryByTestId('page-header-more-menu')).not.toBeInTheDocument();

        // Expert mode: inline button without more-actions trigger
        fireEvent.click(screen.getByTestId('ui-mode-expert'));
        expect(screen.queryByTestId('page-header-more-actions')).not.toBeInTheDocument();
        expect(screen.getByTestId('adv-header-action')).toBeInTheDocument();
    });
});
