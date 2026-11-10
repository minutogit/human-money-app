import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { HelpContentRenderer, renderInlineFormatting } from '../HelpContentRenderer';

describe('HelpContentRenderer', () => {
    it('returns null for empty content', () => {
        const { container } = render(<HelpContentRenderer content="" />);
        expect(container.firstChild).toBeNull();
    });

    it('renders plain paragraphs separated by newlines', () => {
        const content = 'First paragraph here.\n\nSecond paragraph here.';
        render(<HelpContentRenderer content={content} />);

        expect(screen.getByText('First paragraph here.')).toBeInTheDocument();
        expect(screen.getByText('Second paragraph here.')).toBeInTheDocument();
    });

    it('renders inline bold correctly using strong tags without innerHTML', () => {
        const content = 'This is **very important** text.';
        render(<HelpContentRenderer content={content} />);

        const strongEl = screen.getByText('very important');
        expect(strongEl.tagName).toBe('STRONG');
        expect(strongEl).toHaveClass('font-semibold');
    });

    it('renders bullet lists starting with • or -', () => {
        const content = '• Item one\n• Item two\n- Item three';
        render(<HelpContentRenderer content={content} />);

        expect(screen.getByText('Item one')).toBeInTheDocument();
        expect(screen.getByText('Item two')).toBeInTheDocument();
        expect(screen.getByText('Item three')).toBeInTheDocument();
    });

    it('renders mixed paragraphs and bullet lists with inline bold', () => {
        const content = '**Header:**\nThis is intro.\n\n• **Feature A:** Fast\n• **Feature B:** Secure';
        render(<HelpContentRenderer content={content} />);

        expect(screen.getByText('Header:')).toBeInTheDocument();
        expect(screen.getByText('Feature A:')).toBeInTheDocument();
        expect(screen.getByText('Feature B:')).toBeInTheDocument();
    });

    it('renderInlineFormatting handles multiple bold segments and edge cases', () => {
        const nodes = renderInlineFormatting('Hello **A** and **B** world');
        expect(nodes).toHaveLength(5);
        expect(nodes[0]).toBe('Hello ');
        expect(nodes[2]).toBe(' and ');
        expect(nodes[4]).toBe(' world');
    });
});
