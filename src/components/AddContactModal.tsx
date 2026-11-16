// src/components/AddContactModal.tsx
import React, { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { 
    QrCode, 
    ClipboardPaste, 
    FileText, 
    UserPlus, 
    ArrowLeft, 
    RefreshCw 
} from 'lucide-react';
import { ModalShell } from './ui/ModalShell';
import { QrScannerModal } from './ui/QrScannerModal';
import { Button } from './ui/Button';
import { Textarea } from './ui/Textarea';
import { ExpertTabsLayout } from './ui/ExpertTabsLayout';
import { Disclosure } from './ui/Disclosure';
import { readFromClipboard } from '../utils/clipboard';
import { marketService } from '../services/marketService';
import { parseContactCard } from '../utils/humocoUri';
import { notifySyncStatusUpdated } from '../utils/syncEvents';
import { useSession } from '../context/SessionContext';
import { open } from '@tauri-apps/plugin-dialog';
import { readFile } from '@tauri-apps/plugin-fs';
import { logger } from '../utils/log';
import { stringifyError } from '../utils/errorHelper';

export interface AddContactModalProps {
    isOpen: boolean;
    onClose: () => void;
    onOpenManualAdd: (initialDid?: string, initialProfile?: any, initialSyncUrls?: string[]) => void;
    onContactAdded: () => Promise<void> | void;
    onSuccess: (message: string) => void;
    onError: (message: string) => void;
}

type ModalView = 'tiles' | 'qr_scanner' | 'paste_input';

function extractCardPayload(raw: string): string | null {
    const trimmed = raw.trim();
    if (!trimmed) return null;

    const cardUriMatch = trimmed.match(/humoco:\/\/card\?[^\s"'<>)]+/i);
    if (cardUriMatch) {
        return cardUriMatch[0].replace(/[.,;:!\]]+$/, '');
    }
    if (trimmed.includes('?env=')) {
        return trimmed;
    }
    const firstBrace = trimmed.indexOf('{');
    const lastBrace = trimmed.lastIndexOf('}');
    if (firstBrace !== -1 && lastBrace > firstBrace) {
        const candidate = trimmed.slice(firstBrace, lastBrace + 1);
        try {
            const parsed = JSON.parse(candidate);
            if (
                typeof parsed === 'object' &&
                parsed !== null &&
                ('card_wire' in parsed || ('card' in parsed && 'sig' in parsed) || 'envelope' in parsed)
            ) {
                return candidate;
            }
        } catch {
            return null;
        }
    }
    return null;
}

function isCardEnvelopePayload(trimmed: string): boolean {
    return extractCardPayload(trimmed) !== null;
}

export const AddContactModal: React.FC<AddContactModalProps> = ({
    isOpen,
    onClose,
    onOpenManualAdd,
    onContactAdded,
    onSuccess,
    onError,
}) => {
    const { t } = useTranslation();
    const { protectAction } = useSession();
    const [view, setView] = useState<ModalView>('tiles');
    const [pasteText, setPasteText] = useState('');
    const [smartError, setSmartError] = useState<string | null>(null);
    const [isProcessing, setIsProcessing] = useState(false);

    // Reset view whenever modal opens or closes
    useEffect(() => {
        if (!isOpen) {
            setView('tiles');
            setPasteText('');
            setSmartError(null);
            setIsProcessing(false);
        }
    }, [isOpen]);

    const handleImportPayload = useCallback(async (rawPayload: string) => {
        setSmartError(null);
        const trimmed = rawPayload.trim();
        if (!trimmed) {
            const msg = t('contacts.emptyClipboard');
            setSmartError(msg);
            onError(msg);
            return;
        }

        // 1. Check if it's a contact card or DID via canonical parseContactCard
        const parsedContact = parseContactCard(trimmed);
        if (parsedContact && parsedContact.did) {
            let initialProfile: { firstName?: string; lastName?: string } | undefined;
            if (parsedContact.name) {
                const parts = parsedContact.name.trim().split(/\s+/);
                if (parts.length > 1) {
                    initialProfile = {
                        firstName: parts.slice(0, -1).join(' '),
                        lastName: parts[parts.length - 1],
                    };
                } else if (parts.length === 1 && parts[0]) {
                    initialProfile = { firstName: parts[0] };
                }
            }
            onClose();
            if (initialProfile || parsedContact.syncUrls) {
                onOpenManualAdd(parsedContact.did, initialProfile, parsedContact.syncUrls);
            } else {
                onOpenManualAdd(parsedContact.did);
            }
            return;
        }

        // 2. Verify that payload looks like a signed card envelope before attempting import
        const cardPayload = extractCardPayload(trimmed);
        if (!cardPayload) {
            const msg = t('contacts.invalidContactPayload', 'Ungültiges Kontakt- oder Visitenkarten-Format.');
            setSmartError(msg);
            onError(msg);
            return;
        }

        // 3. Import as signed card envelope (.hmcard, JSON, or card URI)
        setIsProcessing(true);
        try {
            const res = await protectAction(async (pwd) => {
                return await marketService.importContactCard(cardPayload, pwd || undefined);
            });

            if (res && res.status === 'accepted') {
                await onContactAdded();
                marketService.probeContactSyncs?.(false)
                    ?.then(() => notifySyncStatusUpdated())
                    ?.catch(() => {});
                onSuccess(
                    res.replaced
                        ? t('marketplace.importUpdatedDesc', 'Visitenkarte erfolgreich aktualisiert.')
                        : t('marketplace.importSuccessDesc', 'Visitenkarte erfolgreich importiert.')
                );
                onClose();
            } else if (res && res.status === 'ignored_stale') {
                const msg = t('marketplace.importIgnoredDesc', 'Diese Visitenkarte ist bereits in gleicher oder neuerer Version gespeichert.');
                setSmartError(msg);
                onError(msg);
            } else if (res && res.status === 'rejected_expired') {
                const msg = t('marketplace.importExpiredDesc', 'Diese Visitenkarte ist abgelaufen.');
                setSmartError(msg);
                onError(msg);
            } else if (res && res.status === 'rejected_hops') {
                const msg = t('marketplace.importHopsDesc', 'Diese Visitenkarte liegt außerhalb der erlaubten Weiterleitungs-Reichweite.');
                setSmartError(msg);
                onError(msg);
            } else if (res && res.status === 'rejected_offender') {
                const msg = t('marketplace.importOffenderDesc', 'Diese Visitenkarte wurde wegen Betrugsverdacht blockiert.');
                setSmartError(msg);
                onError(msg);
            } else {
                const msg = t('contacts.invalidContactPayload', 'Ungültiges Kontakt- oder Visitenkarten-Format.');
                setSmartError(msg);
                onError(msg);
            }
        } catch (err) {
            logger.error(`Import card error: ${stringifyError(err)}`);
            const msg = stringifyError(err);
            setSmartError(msg);
            onError(msg);
        } finally {
            setIsProcessing(false);
        }
    }, [onError, t, onClose, onOpenManualAdd, protectAction, onContactAdded, onSuccess]);

    // Tile 2: Clipboard
    const handleTileClipboard = async () => {
        try {
            const { text, error } = await readFromClipboard();
            if (error || !text || !text.trim()) {
                setPasteText('');
                setView('paste_input');
                return;
            }
            const trimmed = text.trim();
            const parsedContact = parseContactCard(trimmed);
            const isCard = isCardEnvelopePayload(trimmed);
            if (!parsedContact && !isCard) {
                // Clipboard does not contain a valid contact card or envelope: switch to paste input without error
                setPasteText('');
                setView('paste_input');
                return;
            }
            await handleImportPayload(trimmed);
        } catch {
            setPasteText('');
            setView('paste_input');
        }
    };

    // Tile 3: File (.hmcard)
    const handleTileFile = async () => {
        try {
            const selectedPath = await open({
                multiple: false,
                filters: [
                    { name: t('marketplace.cardFilterName', 'Visitenkarte (.hmcard)'), extensions: ['hmcard', 'json'] },
                    { name: t('common.allFiles', 'All Files'), extensions: ['*'] },
                ],
            });

            if (typeof selectedPath === 'string') {
                const fileBytes = await readFile(selectedPath);
                const fileText = new TextDecoder('utf-8').decode(fileBytes);
                await handleImportPayload(fileText);
            }
        } catch (err) {
            logger.error(`Error opening file: ${stringifyError(err)}`);
            onError(stringifyError(err));
        }
    };

    // Smart clipboard fill in Simple Mode
    const handleSmartPasteClipboard = async () => {
        setSmartError(null);
        const { text, error } = await readFromClipboard();
        if (error || !text || !text.trim()) {
            const msg = t('contacts.emptyClipboard', 'Die Zwischenablage ist leer.');
            setSmartError(msg);
            onError(msg);
            return;
        }
        setPasteText(text.trim());
    };

    // Tile 4: Manual (Benutzer-ID)
    const handleTileManual = () => {
        onClose();
        onOpenManualAdd();
    };

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={onClose}
            maxWidth="lg"
            title={
                view === 'tiles' ? (
                    t('contacts.addModalTitle', 'Kontakt hinzufügen')
                ) : (
                    <div className="flex items-center gap-2">
                        <button
                            type="button"
                            onClick={() => setView('tiles')}
                            className="p-1 hover:bg-slate-100 rounded-lg text-theme-light hover:text-theme-primary transition-colors"
                            title={t('common.back', 'Zurück')}
                        >
                            <ArrowLeft size={18} />
                        </button>
                        <span>
                            {view === 'qr_scanner'
                                ? t('contacts.addTileQrTitle', 'QR scannen')
                                : t('contacts.addTileClipboardTitle', 'Link/Code einfügen')}
                        </span>
                    </div>
                )
            }
            description={
                view === 'tiles'
                    ? t('contacts.addModalSubtitle', 'Wähle, wie du den Kontakt hinzufügen möchtest.')
                    : undefined
            }
        >
            {/* View 1: Simple Smart Input vs Expert 4 Tiles */}
            {view === 'tiles' && (
                <ExpertTabsLayout
                    simpleContent={
                        <div className="space-y-4 py-1" data-testid="smart-add-contact-panel">
                            <div className="space-y-2">
                                <label
                                    htmlFor="smart-contact-input"
                                    className="block text-xs font-bold text-theme-secondary"
                                >
                                    {t('contacts.smartAddLabel', 'Kontakt-Link, Benutzer-ID oder Visitenkarte einfügen')}
                                </label>
                                <Textarea
                                    id="smart-contact-input"
                                    rows={3}
                                    value={pasteText}
                                    onChange={(e) => {
                                        setPasteText(e.target.value);
                                        if (smartError) setSmartError(null);
                                    }}
                                    placeholder={t('contacts.smartAddPlaceholder', 'humoco://..., Benutzer-ID oder .hmcard Text hier einfügen...')}
                                    data-testid="smart-contact-input"
                                />
                                {smartError && (
                                    <p className="text-xs font-medium text-rose-600" data-testid="smart-add-error">
                                        {smartError}
                                    </p>
                                )}
                                <div className="flex flex-wrap items-center gap-2 pt-1">
                                    <Button
                                        type="button"
                                        variant="secondary"
                                        size="sm"
                                        onClick={handleSmartPasteClipboard}
                                        disabled={isProcessing}
                                        className="gap-1.5"
                                        data-testid="smart-paste-clipboard-btn"
                                    >
                                        <ClipboardPaste size={15} />
                                        <span>{t('contacts.smartPasteClipboardBtn', 'Aus Zwischenablage')}</span>
                                    </Button>
                                    <Button
                                        type="button"
                                        variant="primary"
                                        size="sm"
                                        onClick={() => handleImportPayload(pasteText)}
                                        disabled={isProcessing || !pasteText.trim()}
                                        className="gap-1.5 ml-auto"
                                        data-testid="smart-contact-submit"
                                    >
                                        <span>{isProcessing ? t('common.importing', 'Wird übernommen...') : t('contacts.smartSubmitBtn', 'Übernehmen')}</span>
                                    </Button>
                                </div>
                            </div>

                            <div className="pt-2 border-t border-theme-subtle/60">
                                <Button
                                    type="button"
                                    variant="secondary"
                                    onClick={() => setView('qr_scanner')}
                                    className="w-full gap-2 py-2.5"
                                    data-testid="smart-add-qr-btn"
                                >
                                    <QrCode size={18} />
                                    <span>{t('contacts.addTileQrTitle', 'QR scannen')}</span>
                                </Button>
                            </div>

                            <Disclosure
                                title={t('contacts.otherWaysLabel', 'Andere Wege')}
                                defaultOpen={false}
                                toggleTestId="add-contact-other-ways-toggle"
                                contentTestId="add-contact-other-ways-content"
                            >
                                <div className="flex flex-col sm:flex-row gap-2">
                                    <Button
                                        type="button"
                                        variant="secondary"
                                        size="sm"
                                        onClick={handleTileFile}
                                        className="flex-1 gap-2 justify-start"
                                        data-testid="add-other-file"
                                    >
                                        <FileText size={15} />
                                        <span>{t('contacts.otherWayFileBtn', 'Visitenkarte aus Datei öffnen')}</span>
                                    </Button>
                                    <Button
                                        type="button"
                                        variant="secondary"
                                        size="sm"
                                        onClick={handleTileManual}
                                        className="flex-1 gap-2 justify-start"
                                        data-testid="add-other-manual"
                                    >
                                        <UserPlus size={15} />
                                        <span>{t('contacts.otherWayManualBtn', 'Manuell eintragen')}</span>
                                    </Button>
                                </div>
                            </Disclosure>
                        </div>
                    }
                    expertContent={
                        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 py-2" data-testid="add-contact-tiles">
                            {/* Tile 1: QR scannen */}
                            <button
                                type="button"
                                data-testid="add-tile-qr"
                                onClick={() => setView('qr_scanner')}
                                className="group p-5 rounded-2xl border border-theme-subtle bg-white hover:border-theme-primary/40 hover:bg-theme-primary/5 text-left transition-all flex flex-col gap-3 shadow-xs cursor-pointer focus:outline-none focus:ring-2 focus:ring-theme-primary/20"
                            >
                                <div className="w-12 h-12 rounded-2xl bg-sky-50 text-sky-600 border border-sky-100 flex items-center justify-center shrink-0 group-hover:scale-105 transition-transform">
                                    <QrCode size={24} />
                                </div>
                                <div className="space-y-1">
                                    <h4 className="font-extrabold text-sm text-theme-primary tracking-tight group-hover:text-theme-accent transition-colors">
                                        {t('contacts.addTileQrTitle', 'QR scannen')}
                                    </h4>
                                    <p className="text-xs text-theme-light leading-relaxed">
                                        {t('contacts.addTileQrSubtitle', 'Kamera nutzen, um eine Visitenkarte zu scannen')}
                                    </p>
                                </div>
                            </button>

                            {/* Tile 2: Link/Code einfügen */}
                            <button
                                type="button"
                                data-testid="add-tile-clipboard"
                                onClick={handleTileClipboard}
                                className="group p-5 rounded-2xl border border-theme-subtle bg-white hover:border-theme-primary/40 hover:bg-theme-primary/5 text-left transition-all flex flex-col gap-3 shadow-xs cursor-pointer focus:outline-none focus:ring-2 focus:ring-theme-primary/20"
                            >
                                <div className="w-12 h-12 rounded-2xl bg-amber-50 text-amber-600 border border-amber-100 flex items-center justify-center shrink-0 group-hover:scale-105 transition-transform">
                                    <ClipboardPaste size={24} />
                                </div>
                                <div className="space-y-1">
                                    <h4 className="font-extrabold text-sm text-theme-primary tracking-tight group-hover:text-theme-accent transition-colors">
                                        {t('contacts.addTileClipboardTitle', 'Link/Code einfügen')}
                                    </h4>
                                    <p className="text-xs text-theme-light leading-relaxed">
                                        {t('contacts.addTileClipboardSubtitle', 'Aus der Zwischenablage einfügen')}
                                    </p>
                                </div>
                            </button>

                            {/* Tile 3: Datei öffnen (.hmcard) */}
                            <button
                                type="button"
                                data-testid="add-tile-file"
                                onClick={handleTileFile}
                                className="group p-5 rounded-2xl border border-theme-subtle bg-white hover:border-theme-primary/40 hover:bg-theme-primary/5 text-left transition-all flex flex-col gap-3 shadow-xs cursor-pointer focus:outline-none focus:ring-2 focus:ring-theme-primary/20"
                            >
                                <div className="w-12 h-12 rounded-2xl bg-emerald-50 text-emerald-600 border border-emerald-100 flex items-center justify-center shrink-0 group-hover:scale-105 transition-transform">
                                    <FileText size={24} />
                                </div>
                                <div className="space-y-1">
                                    <h4 className="font-extrabold text-sm text-theme-primary tracking-tight group-hover:text-theme-accent transition-colors">
                                        {t('contacts.addTileFileTitle', 'Datei öffnen (.hmcard)')}
                                    </h4>
                                    <p className="text-xs text-theme-light leading-relaxed">
                                        {t('contacts.addTileFileSubtitle', 'Visitenkarte aus einer Datei importieren')}
                                    </p>
                                </div>
                            </button>

                            {/* Tile 4: Manuell (Benutzer-ID) */}
                            <button
                                type="button"
                                data-testid="add-tile-manual"
                                onClick={handleTileManual}
                                className="group p-5 rounded-2xl border border-theme-subtle bg-white hover:border-theme-primary/40 hover:bg-theme-primary/5 text-left transition-all flex flex-col gap-3 shadow-xs cursor-pointer focus:outline-none focus:ring-2 focus:ring-theme-primary/20"
                            >
                                <div className="w-12 h-12 rounded-2xl bg-indigo-50 text-indigo-600 border border-indigo-100 flex items-center justify-center shrink-0 group-hover:scale-105 transition-transform">
                                    <UserPlus size={24} />
                                </div>
                                <div className="space-y-1">
                                    <h4 className="font-extrabold text-sm text-theme-primary tracking-tight group-hover:text-theme-accent transition-colors">
                                        {t('contacts.addTileManualTitle', 'Manuell (Benutzer-ID)')}
                                    </h4>
                                    <p className="text-xs text-theme-light leading-relaxed">
                                        {t('contacts.addTileManualSubtitle', 'Benutzer-ID direkt von Hand eingeben')}
                                    </p>
                                </div>
                            </button>
                        </div>
                    }
                />
            )}

            {/* View 2: QR Scanner */}
            <QrScannerModal
                isOpen={isOpen && view === 'qr_scanner'}
                onClose={() => setView('tiles')}
                onScan={(raw) => {
                    setView('tiles');
                    void handleImportPayload(raw);
                }}
                title={t('contacts.addTileQrTitle', 'QR scannen')}
            />

            {/* View 3: Paste Input */}
            {view === 'paste_input' && (
                <div className="space-y-4 py-2" data-testid="paste-input-view">
                    <div className="space-y-1.5">
                        <label className="block text-xs font-bold text-theme-secondary">
                            {t('contacts.pasteInputLabel', 'Link, Benutzer-ID oder Visitenkarten-Code einfügen')}
                        </label>
                        <Textarea
                            rows={4}
                            value={pasteText}
                            onChange={(e) => setPasteText(e.target.value)}
                            placeholder={t(
                                'contacts.pasteInputPlaceholder',
                                'Füge hier den humoco://-Link, die Benutzer-ID oder den Code der Visitenkarte ein...'
                            )}
                            className="w-full font-mono text-xs"
                            autoFocus
                        />
                    </div>

                    <div className="flex items-center justify-between gap-3 pt-2">
                        <button
                            type="button"
                            onClick={() => setView('tiles')}
                            className="text-xs font-bold text-theme-light hover:text-theme-primary transition-colors"
                        >
                            ← {t('common.back', 'Zurück zur Auswahl')}
                        </button>
                        <div className="flex items-center gap-2">
                            <Button
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={async () => {
                                    const { text } = await readFromClipboard();
                                    if (text) setPasteText(text);
                                }}
                            >
                                {t('contacts.pasteFromClipboardBtn', 'Aus Zwischenablage')}
                            </Button>
                            <Button
                                type="button"
                                size="sm"
                                disabled={isProcessing || !pasteText.trim()}
                                onClick={() => handleImportPayload(pasteText)}
                                className="gap-1.5"
                            >
                                {isProcessing && <RefreshCw size={13} className="animate-spin" />}
                                <span>{t('contacts.importCardBtn', 'Übernehmen')}</span>
                            </Button>
                        </div>
                    </div>
                </div>
            )}
        </ModalShell>
    );
};

export default AddContactModal;
