import { useState, useRef, useEffect } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { HelpCircle } from 'lucide-react';
import { HelpModal } from './HelpModal';

export type HelpTopic = 'mnemonic' | 'passphrase' | 'subaccount' | 'profile' | 'handover' | 'senderName';

interface HelpIconProps {
    topic: HelpTopic;
    className?: string;
    size?: number;
}

export function HelpIcon({ topic, className = '', size = 16 }: HelpIconProps) {
    const { t } = useTranslation();
    const [isHovered, setIsHovered] = useState(false);
    const [isModalOpen, setIsModalOpen] = useState(false);
    const [coords, setCoords] = useState<{ top: number; left: number } | null>(null);
    const buttonRef = useRef<HTMLButtonElement>(null);

    // Map topics to their respective i18n keys based on the domains
    const getKeys = () => {
        switch (topic) {
            case 'mnemonic':
                return {
                    title: t('profile.helpMnemonicTitle'),
                    short: t('profile.helpMnemonicShort'),
                    long: t('profile.helpMnemonicLong')
                };
            case 'passphrase':
                return {
                    title: t('profile.helpPassphraseTitle'),
                    short: t('profile.helpPassphraseShort'),
                    long: t('profile.helpPassphraseLong')
                };
            case 'subaccount':
                return {
                    title: t('profile.helpSubaccountTitle'),
                    short: t('profile.helpSubaccountShort'),
                    long: t('profile.helpSubaccountLong')
                };
            case 'profile':
                return {
                    title: t('auth.helpProfileTitle'),
                    short: t('auth.helpProfileShort'),
                    long: t('auth.helpProfileLong')
                };
            case 'handover':
                return {
                    title: t('auth.helpHandoverTitle'),
                    short: t('auth.helpHandoverShort'),
                    long: t('auth.helpHandoverLong')
                };
            case 'senderName':
                return {
                    title: t('transfer.helpSenderNameTitle'),
                    short: t('transfer.helpSenderNameShort'),
                    long: t('transfer.helpSenderNameLong')
                };
        }
    };

    const keys = getKeys();

    const updateCoords = () => {
        if (buttonRef.current) {
            const rect = buttonRef.current.getBoundingClientRect();
            setCoords({
                top: rect.top + window.scrollY,
                left: rect.left + rect.width / 2 + window.scrollX
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
                    aria-label={keys.title}
                >
                    <HelpCircle size={size} />
                </button>
            </div>

            {isHovered && coords && createPortal(
                <div 
                    style={{
                        position: 'absolute',
                        top: `${coords.top}px`,
                        left: `${coords.left}px`,
                        transform: 'translate(-50%, -100%)',
                        marginTop: '-8px',
                    }}
                    className="w-64 bg-slate-900 text-white text-xs rounded-xl p-3 shadow-lg z-[9999] pointer-events-none animate-in fade-in zoom-in-95 duration-200 text-center"
                >
                    <p className="font-semibold mb-1">{keys.title}</p>
                    <p className="opacity-90">{keys.short}</p>
                    <div className="absolute top-full left-1/2 -translate-x-1/2 border-4 border-transparent border-t-slate-900" />
                </div>,
                document.body
            )}

            <HelpModal
                isOpen={isModalOpen}
                onClose={() => setIsModalOpen(false)}
                title={keys.title}
                short={keys.short}
                long={keys.long}
            />
        </>
    );
}
