// src/help/HelpContentRenderer.tsx
import React from 'react';

interface HelpContentRendererProps {
    content: string;
    className?: string;
}

/**
 * Parses inline bold syntax `**bold text**` safely into React nodes
 * without using innerHTML or dangerouslySetInnerHTML.
 */
export function renderInlineFormatting(text: string): React.ReactNode[] {
    const parts = text.split(/(\*\*.*?\*\*)/g);
    return parts.map((part, index) => {
        if (part.startsWith('**') && part.endsWith('**') && part.length >= 4) {
            return (
                <strong key={index} className="font-semibold text-theme-secondary">
                    {part.slice(2, -2)}
                </strong>
            );
        }
        return part;
    });
}

/**
 * Renders markdown-light text formatted with paragraphs (\n\n), bullet lists (• / -),
 * and inline bold (**text**), strictly avoiding dangerouslySetInnerHTML.
 */
export const HelpContentRenderer: React.FC<HelpContentRendererProps> = ({ content, className = '' }) => {
    if (!content) return null;

    const blocks = content.split(/\n\n+/);

    return (
        <div className={`space-y-3.5 ${className}`}>
            {blocks.map((block, bIdx) => {
                const lines = block
                    .split('\n')
                    .map((l) => l.trim())
                    .filter(Boolean);

                const hasListItems = lines.some((l) => l.startsWith('• ') || l.startsWith('- '));

                if (hasListItems) {
                    return (
                        <div key={bIdx} className="space-y-1.5 my-1">
                            {lines.map((line, lIdx) => {
                                const isListItem = line.startsWith('• ') || line.startsWith('- ');
                                if (isListItem) {
                                    const cleanText = line.replace(/^[•\-]\s*/, '');
                                    return (
                                        <div
                                            key={lIdx}
                                            className="flex items-start gap-2.5 text-sm text-theme-secondary leading-relaxed pl-1"
                                        >
                                            <span className="text-theme-primary select-none mt-1 text-xs shrink-0">•</span>
                                            <span className="flex-1">{renderInlineFormatting(cleanText)}</span>
                                        </div>
                                    );
                                }
                                return (
                                    <p key={lIdx} className="text-sm text-theme-secondary leading-relaxed">
                                        {renderInlineFormatting(line)}
                                    </p>
                                );
                            })}
                        </div>
                    );
                }

                return (
                    <p key={bIdx} className="text-sm text-theme-secondary leading-relaxed">
                        {renderInlineFormatting(block)}
                    </p>
                );
            })}
        </div>
    );
};
