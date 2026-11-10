// src/components/ui/HelpIcon.tsx
import { useState, useRef, useEffect } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { HelpCircle } from 'lucide-react';
import { HelpModal } from './HelpModal';
import { HELP_TOPICS, HelpTopicId } from '../../help/helpTopics';

export type HelpTopic = HelpTopicId;

interface HelpIconProps {
    topic: HelpTopicId;
    className?: string;
    size?: number;
}

export function HelpIcon({ topic, className = '', size = 16 }: HelpIconProps) {
    const { t } = useTranslation();
    const [isHovered, setIsHovered] = useState(false);
    const [isModalOpen, setIsModalOpen] = useState(false);
    const [coords, setCoords] = useState<{
        top: number;
        left: number;
        arrowOffset: number;
        placement: 'top' | 'bottom';
    } | null>(null);
    const buttonRef = useRef<HTMLButtonElement>(null);

    const topicDef = HELP_TOPICS[topic];
    const title = topicDef ? t(topicDef.titleKey) : topic;
    const short = topicDef ? t(topicDef.shortKey) : '';

    const updateCoords = () => {
        if (buttonRef.current) {
            const rect = buttonRef.current.getBoundingClientRect();
            const placement: 'top' | 'bottom' = rect.top < 190 ? 'bottom' : 'top';
            const innerWidth = typeof window !== 'undefined' ? window.innerWidth : 1024;
            const margin = Math.min(144, innerWidth / 2 - 12);
            const buttonCenter = rect.left + rect.width / 2;
            const clampedLeft = Math.max(margin + 12, Math.min(buttonCenter, innerWidth - margin - 12));
            const arrowOffset = Math.max(-margin + 16, Math.min(buttonCenter - clampedLeft, margin - 16));

            setCoords({
                top: placement === 'bottom' ? rect.bottom + 8 : rect.top - 8,
                left: clampedLeft,
                arrowOffset,
                placement,
            });
        }
    };

    useEffect(() => {
        if (!isHovered) return;
        updateCoords();
        window.addEventListener('resize', updateCoords);
        window.addEventListener('scroll', updateCoords, true);
        return () => {
            window.removeEventListener('resize', updateCoords);
            window.removeEventListener('scroll', updateCoords, true);
        };
    }, [isHovered]);

    const handleClick = (e: React.MouseEvent) => {
        e.preventDefault();
        e.stopPropagation();
        setIsHovered(false);
        setIsModalOpen(true);
    };

    return (
        <>
            <div className={`relative inline-flex items-center leading-none ${className}`}>
                <button
                    ref={buttonRef}
                    type="button"
                    className="text-theme-light hover:text-theme-primary transition-colors focus:outline-none p-0.5 rounded-full hover:bg-theme-primary/5 cursor-pointer"
                    onClick={handleClick}
                    onMouseEnter={() => setIsHovered(true)}
                    onMouseLeave={() => setIsHovered(false)}
                    onFocus={() => setIsHovered(true)}
                    onBlur={() => setIsHovered(false)}
                    aria-label={title}
                >
                    <HelpCircle size={size} />
                </button>
            </div>

            {isHovered && coords && typeof document !== 'undefined' && document.body && createPortal(
                <div
                    style={{
                        position: 'fixed',
                        top: `${coords.top}px`,
                        left: `${coords.left}px`,
                        transform: coords.placement === 'bottom' ? 'translate(-50%, 0)' : 'translate(-50%, -100%)',
                    }}
                    className="w-72 max-w-[calc(100vw-24px)] bg-slate-900 text-white text-xs rounded-xl p-3 shadow-lg z-[9999] pointer-events-none animate-in fade-in zoom-in-95 duration-200 text-left leading-relaxed"
                >
                    <p className="font-semibold mb-1 text-slate-100">{title}</p>
                    <p className="opacity-90">{short}</p>
                    {coords.placement === 'bottom' ? (
                        <div
                            style={{ left: `calc(50% + ${coords.arrowOffset}px)` }}
                            className="absolute bottom-full -translate-x-1/2 border-4 border-transparent border-b-slate-900"
                        />
                    ) : (
                        <div
                            style={{ left: `calc(50% + ${coords.arrowOffset}px)` }}
                            className="absolute top-full -translate-x-1/2 border-4 border-transparent border-t-slate-900"
                        />
                    )}
                </div>,
                document.body
            )}

            <HelpModal
                isOpen={isModalOpen}
                onClose={() => setIsModalOpen(false)}
                topic={topic}
            />
        </>
    );
}
