import { useState, useRef, useEffect } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { HelpCircle } from 'lucide-react';
import { HelpModal } from './HelpModal';

export type HelpTopic =
    | 'mnemonic'
    | 'passphrase'
    | 'subaccount'
    | 'profile'
    | 'handover'
    | 'senderName'
    | 'privacy'
    | 'voucherMode'
    | 'voucherCreator'
    | 'voucherSignatures'
    | 'signatureRequest'
    | 'wotNetwork'
    | 'voucherTrust'
    | 'wotReach'
    | 'settingsPrivacy'
    | 'posMode'
    | 'exclusiveGateways'
    | 'gatewayTokens'
    | 'deviceBinding';

interface HelpIconProps {
    topic: HelpTopic;
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
            case 'privacy':
                return {
                    title: t('transfer.helpPrivacyTitle'),
                    short: t('transfer.helpPrivacyShort'),
                    long: t('transfer.helpPrivacyLong')
                };
            case 'voucherMode':
                return {
                    title: t('voucher.helpModeTitle'),
                    short: t('voucher.helpModeShort'),
                    long: t('voucher.helpModeLong')
                };
            case 'voucherCreator':
                return {
                    title: t('voucher.helpCreatorTitle'),
                    short: t('voucher.helpCreatorShort'),
                    long: t('voucher.helpCreatorLong')
                };
            case 'voucherSignatures':
                return {
                    title: t('voucher.helpSignaturesTitle'),
                    short: t('voucher.helpSignaturesShort'),
                    long: t('voucher.helpSignaturesLong')
                };
            case 'signatureRequest':
                return {
                    title: t('voucher.helpSignRequestTitle'),
                    short: t('voucher.helpSignRequestShort'),
                    long: t('voucher.helpSignRequestLong')
                };
            case 'wotNetwork':
                return {
                    title: t('contacts.helpWotTitle'),
                    short: t('contacts.helpWotShort'),
                    long: t('contacts.helpWotLong')
                };
            case 'voucherTrust':
                return {
                    title: t('voucher.helpTrustTitle'),
                    short: t('voucher.helpTrustShort'),
                    long: t('voucher.helpTrustLong')
                };
            case 'wotReach':
                return {
                    title: t('contacts.helpWotReachTitle'),
                    short: t('contacts.helpWotReachShort'),
                    long: t('contacts.helpWotReachLong')
                };
            case 'settingsPrivacy':
                return {
                    title: t('settings.helpPrivacyTitle'),
                    short: t('settings.helpPrivacyShort'),
                    long: t('settings.helpPrivacyLong')
                };
            case 'posMode':
                return {
                    title: t('settings.helpPosModeTitle'),
                    short: t('settings.helpPosModeShort'),
                    long: t('settings.helpPosModeLong')
                };
            case 'exclusiveGateways':
                return {
                    title: t('settings.helpExclusiveGatewaysTitle'),
                    short: t('settings.helpExclusiveGatewaysShort'),
                    long: t('settings.helpExclusiveGatewaysLong')
                };
            case 'gatewayTokens':
                return {
                    title: t('settings.helpGatewayTokensTitle'),
                    short: t('settings.helpGatewayTokensShort'),
                    long: t('settings.helpGatewayTokensLong')
                };
            case 'deviceBinding':
                return {
                    title: t('settings.helpDeviceBindingTitle'),
                    short: t('settings.helpDeviceBindingShort'),
                    long: t('settings.helpDeviceBindingLong')
                };
        }
    };

    const keys = getKeys();

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
                    aria-label={keys.title}
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
                    <p className="font-semibold mb-1 text-slate-100">{keys.title}</p>
                    <p className="opacity-90">{keys.short}</p>
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
                title={keys.title}
                short={keys.short}
                long={keys.long}
            />
        </>
    );
}
