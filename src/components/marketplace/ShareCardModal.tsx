// src/components/marketplace/ShareCardModal.tsx
import { useTranslation } from 'react-i18next';
import { Share2 } from 'lucide-react';
import { ModalShell } from '../ui/ModalShell';
import { Button } from '../ui/Button';
import { ShareCardPanel } from '../profile/ShareCardPanel';

export interface ShareCardModalProps {
    isOpen: boolean;
    onClose: () => void;
    title?: string;
    subtitle?: string;
    fileWire?: string | null;
    shareUri?: string | null;
    qrSvg?: string | null;
    did?: string;
    qrIsPartial?: boolean;
}

export function ShareCardModal({
    isOpen,
    onClose,
    title,
    subtitle,
    fileWire,
    shareUri,
    qrSvg,
    did,
    qrIsPartial = false,
}: ShareCardModalProps) {
    const { t } = useTranslation();

    const modalTitle = title || t('marketplace.shareCardTitle', 'Visitenkarte teilen');
    const modalSubtitle = subtitle || t('marketplace.shareCardSubtitle', 'Teilen per QR-Code, Datei oder Link');

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={onClose}
            maxWidth="md"
            title={
                <div className="flex items-center gap-2.5">
                    <div className="w-9 h-9 rounded-xl bg-theme-primary/10 text-theme-primary flex items-center justify-center">
                        <Share2 size={18} />
                    </div>
                    <div>
                        <h2 className="text-base font-bold text-theme-secondary">{modalTitle}</h2>
                        <p className="text-xs text-theme-light">{modalSubtitle}</p>
                    </div>
                </div>
            }
            footer={
                <div className="flex justify-end w-full">
                    <Button variant="secondary" onClick={onClose} size="sm">
                        {t('common.close', 'Schließen')}
                    </Button>
                </div>
            }
        >
            <ShareCardPanel
                fileWire={fileWire}
                shareUri={shareUri}
                qrSvg={qrSvg}
                did={did}
                qrIsPartial={qrIsPartial}
                qrSize={224}
                resetKey={isOpen}
                className="space-y-5 pt-2"
            />
        </ModalShell>
    );
}
