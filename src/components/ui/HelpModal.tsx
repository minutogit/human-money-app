// src/components/ui/HelpModal.tsx
import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from './Button';
import { ModalShell } from './ModalShell';
import { HelpCircle, ExternalLink } from 'lucide-react';
import { HELP_TOPICS, HelpTopicId, HELP_GROUPS } from '../../help/helpTopics';
import { HelpContentRenderer } from '../../help/HelpContentRenderer';
import { AdvancedDisclosure } from './AdvancedDisclosure';

export interface HelpModalProps {
    isOpen: boolean;
    onClose: () => void;
    topic: HelpTopicId;
    onNavigateToHelpCenter?: (topic: HelpTopicId) => void;
}

export function HelpModal({ isOpen, onClose, topic, onNavigateToHelpCenter }: HelpModalProps) {
    const { t } = useTranslation();
    const [activeTopicId, setActiveTopicId] = useState<HelpTopicId>(topic);

    // Keep active topic in sync with prop when opening
    useEffect(() => {
        if (isOpen) {
            setActiveTopicId(topic);
        }
    }, [isOpen, topic]);

    const currentTopic = HELP_TOPICS[activeTopicId] || HELP_TOPICS[topic];
    if (!currentTopic) return null;

    const title = t(currentTopic.titleKey);
    const short = t(currentTopic.shortKey);
    const long = t(currentTopic.longKey);
    const relatedTopics = currentTopic.related || [];
    const groupDef = HELP_GROUPS.find((g) => g.id === currentTopic.group);
    const groupLabel = groupDef ? t(groupDef.labelKey) : currentTopic.group;

    const handleOpenInHelpCenter = () => {
        onClose();
        if (onNavigateToHelpCenter) {
            onNavigateToHelpCenter(activeTopicId);
        }
        if (typeof window !== 'undefined') {
            window.dispatchEvent(
                new CustomEvent('humoco:navigate-help-center', {
                    detail: { topic: activeTopicId },
                })
            );
        }
    };

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={onClose}
            hideCloseButton
            maxWidth="xl"
            zIndexClass="z-[110]"
            className="bg-white dark:bg-gray-900 rounded-[32px] max-h-[85vh]"
            title={
                <div className="flex items-center gap-3.5 min-w-0">
                    <div className="p-3 bg-theme-primary/10 rounded-2xl text-theme-primary shrink-0">
                        <HelpCircle size={26} />
                    </div>
                    <div className="min-w-0">
                        <span className="inline-block px-2.5 py-0.5 rounded-full text-[10px] font-bold tracking-wider uppercase bg-theme-primary/10 text-theme-primary mb-1">
                            {groupLabel}
                        </span>
                        <h2 className="text-lg font-black text-theme-secondary tracking-tight leading-snug truncate">
                            {title}
                        </h2>
                    </div>
                </div>
            }
            footer={
                <div className="flex flex-wrap items-center justify-between gap-3">
                    <button
                        type="button"
                        onClick={handleOpenInHelpCenter}
                        className="text-xs font-bold text-theme-primary hover:text-theme-primary/80 flex items-center gap-1.5 cursor-pointer hover:underline transition-colors py-2 px-1"
                    >
                        <ExternalLink size={14} />
                        {t('helpCenter.openInHelpCenter')}
                    </button>
                    <Button onClick={onClose} variant="secondary" className="px-6 py-2 rounded-xl text-xs font-bold shadow-sm">
                        {t('common.close')}
                    </Button>
                </div>
            }
        >
            <div className="space-y-6 py-2">
                {/* Highlighted short summary */}
                <div className="text-sm font-medium text-theme-secondary leading-relaxed bg-theme-primary/5 p-4 rounded-2xl border border-theme-primary/10">
                    {short}
                </div>

                {/* Detailed long description */}
                <HelpContentRenderer content={long} />

                {/* Deep dive collapsible */}
                {currentTopic.deepDiveKey && (
                    <AdvancedDisclosure
                        title={
                            <span className="text-xs font-bold text-theme-primary">
                                {t('helpCenter.deepDiveTitle', 'Mehr erfahren / Technischer Hintergrund')}
                            </span>
                        }
                        contentTestId="help-deep-dive"
                        className="bg-slate-50 border border-theme-subtle rounded-2xl overflow-hidden"
                        buttonClassName="w-full flex items-center justify-between gap-2 p-4 text-left hover:bg-slate-100/70 transition-colors cursor-pointer"
                        contentClassName="px-4 pb-4 pt-3 border-t border-theme-subtle/50 text-xs text-theme-secondary leading-relaxed"
                    >
                        <HelpContentRenderer content={t(currentTopic.deepDiveKey)} />
                    </AdvancedDisclosure>
                )}

                {/* Related topics chips */}
                {relatedTopics.length > 0 && (
                    <div className="pt-4 border-t border-theme-subtle/50 space-y-2.5">
                        <span className="text-xs font-bold uppercase tracking-wider text-theme-light">
                            {t('helpCenter.relatedTopics')}
                        </span>
                        <div className="flex flex-wrap gap-2">
                            {relatedTopics.map((relId) => {
                                const relTopic = HELP_TOPICS[relId];
                                if (!relTopic) return null;
                                return (
                                    <button
                                        key={relId}
                                        type="button"
                                        onClick={() => setActiveTopicId(relId)}
                                        className="px-3 py-1.5 rounded-xl text-xs font-semibold bg-theme-subtle/20 hover:bg-theme-primary/10 hover:text-theme-primary text-theme-secondary transition-all cursor-pointer border border-theme-subtle/40"
                                    >
                                        {t(relTopic.titleKey)}
                                    </button>
                                );
                            })}
                        </div>
                    </div>
                )}
            </div>
        </ModalShell>
    );
}
