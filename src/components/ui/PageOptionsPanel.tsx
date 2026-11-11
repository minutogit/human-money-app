// src/components/ui/PageOptionsPanel.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import { X } from 'lucide-react';
import { HelpIcon } from './HelpIcon';
import { HelpTopicId } from '../../help/helpTopics';

export interface PageOptionsPanelProps {
    title?: React.ReactNode;
    icon?: React.ReactNode;
    helpTopic?: HelpTopicId;
    headerRight?: React.ReactNode;
    onClose?: () => void;
    children: React.ReactNode;
    className?: string;
    'data-testid'?: string;
}

export function PageOptionsPanel({
    title,
    icon,
    helpTopic,
    headerRight,
    onClose,
    children,
    className = '',
    'data-testid': testId = 'page-options-panel',
}: PageOptionsPanelProps) {
    const { t } = useTranslation();
    const hasHeader = Boolean(title || icon || helpTopic || headerRight || onClose);

    return (
        <section
            data-testid={testId}
            className={`bg-white/75 backdrop-blur-md border border-theme-subtle rounded-2xl p-4 sm:p-5 shadow-sm space-y-4 animate-in fade-in slide-in-from-top-2 duration-200 ${className}`.trim()}
        >
            {hasHeader && (
                <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pb-3 border-b border-theme-subtle/40">
                    <div className="flex items-center gap-2 flex-wrap">
                        {icon && <span className="text-theme-primary shrink-0">{icon}</span>}
                        {title && (
                            typeof title === 'string' ? (
                                <h4 className="text-sm font-black text-theme-secondary tracking-tight">
                                    {title}
                                </h4>
                            ) : (
                                title
                            )
                        )}
                        {helpTopic && <HelpIcon topic={helpTopic} size={15} />}
                    </div>
                    {(headerRight || onClose) && (
                        <div className="flex items-center gap-2 self-start sm:self-auto flex-wrap">
                            {headerRight}
                            {onClose && (
                                <button
                                    type="button"
                                    onClick={onClose}
                                    className="p-1 rounded-lg text-theme-light hover:text-theme-secondary hover:bg-theme-subtle/30 transition-colors cursor-pointer"
                                    aria-label={t('common.close', 'Schließen')}
                                    title={t('common.close', 'Schließen')}
                                >
                                    <X size={16} />
                                </button>
                            )}
                        </div>
                    )}
                </div>
            )}
            {children}
        </section>
    );
}

export default PageOptionsPanel;
