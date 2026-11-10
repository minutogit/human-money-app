// src/components/ui/InfoHint.tsx
import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Info } from 'lucide-react';
import { HELP_TOPICS, HelpTopicId } from '../../help/helpTopics';
import { HelpModal } from './HelpModal';

export interface InfoHintProps {
    topic: HelpTopicId;
    variant?: 'inline' | 'banner';
    className?: string;
}

export const InfoHint: React.FC<InfoHintProps> = ({
    topic,
    variant = 'inline',
    className = '',
}) => {
    const { t } = useTranslation();
    const [isModalOpen, setIsModalOpen] = useState(false);

    const topicDef = HELP_TOPICS[topic];
    if (!topicDef) return null;

    const shortText = t(topicDef.shortKey);

    if (variant === 'banner') {
        return (
            <>
                <div
                    className={`p-3.5 rounded-2xl bg-theme-primary/5 border border-theme-primary/10 flex items-start gap-3 text-xs leading-relaxed ${className}`}
                >
                    <Info size={16} className="text-theme-primary shrink-0 mt-0.5" />
                    <div className="flex-1 text-theme-secondary">
                        <span>{shortText}</span>
                        <button
                            type="button"
                            onClick={() => setIsModalOpen(true)}
                            className="ml-1.5 font-bold text-theme-primary hover:text-theme-primary/80 hover:underline inline-flex items-center gap-0.5 cursor-pointer transition-colors focus:outline-none"
                        >
                            {t('helpCenter.learnMore')} →
                        </button>
                    </div>
                </div>

                <HelpModal
                    isOpen={isModalOpen}
                    onClose={() => setIsModalOpen(false)}
                    topic={topic}
                />
            </>
        );
    }

    return (
        <>
            <div
                className={`inline-flex items-center flex-wrap gap-1.5 text-xs text-theme-secondary leading-normal ${className}`}
            >
                <Info size={14} className="text-theme-primary shrink-0" />
                <span>{shortText}</span>
                <button
                    type="button"
                    onClick={() => setIsModalOpen(true)}
                    className="font-bold text-theme-primary hover:text-theme-primary/80 hover:underline inline-flex items-center gap-0.5 cursor-pointer transition-colors focus:outline-none whitespace-nowrap"
                >
                    {t('helpCenter.learnMore')} →
                </button>
            </div>

            <HelpModal
                isOpen={isModalOpen}
                onClose={() => setIsModalOpen(false)}
                topic={topic}
            />
        </>
    );
};
