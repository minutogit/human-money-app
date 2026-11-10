// src/components/AddContactModal.tsx
import React, { useState, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { 
    QrCode, 
    ClipboardPaste, 
    FileText, 
    UserPlus, 
    ArrowLeft, 
    Camera, 
    RefreshCw 
} from 'lucide-react';
import { ModalShell } from './ui/ModalShell';
import { Button } from './ui/Button';
import { Textarea } from './ui/Textarea';
import { readFromClipboard } from '../utils/clipboard';
import { marketService } from '../services/marketService';
import { parseContactCard } from '../utils/humocoUri';
import { useSession } from '../context/SessionContext';
import { open } from '@tauri-apps/plugin-dialog';
import { readFile } from '@tauri-apps/plugin-fs';
import { logger } from '../utils/log';
import { stringifyError } from '../utils/errorHelper';

export interface AddContactModalProps {
    isOpen: boolean;
    onClose: () => void;
    onOpenManualAdd: (initialDid?: string, initialProfile?: any) => void;
    onContactAdded: () => Promise<void> | void;
    onSuccess: (message: string) => void;
    onError: (message: string) => void;
}

type ModalView = 'tiles' | 'qr_scanner' | 'paste_input';

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
    const [isProcessing, setIsProcessing] = useState(false);
    const [cameraError, setCameraError] = useState<string | null>(null);
    const videoRef = useRef<HTMLVideoElement>(null);
    const streamRef = useRef<MediaStream | null>(null);

    // Reset view whenever modal opens or closes
    useEffect(() => {
        if (!isOpen) {
            stopCamera();
            setView('tiles');
            setPasteText('');
            setCameraError(null);
            setIsProcessing(false);
        }
    }, [isOpen]);

    const stopCamera = () => {
        if (streamRef.current) {
            streamRef.current.getTracks().forEach((track) => track.stop());
            streamRef.current = null;
        }
    };

    useEffect(() => {
        if (view === 'qr_scanner') {
            startCamera();
        } else {
            stopCamera();
        }
        return () => {
            stopCamera();
        };
    }, [view]);

    const startCamera = async () => {
        setCameraError(null);
        if (typeof navigator === 'undefined' || !navigator.mediaDevices?.getUserMedia) {
            setCameraError(t('contacts.cameraNotSupported', 'Kamera auf diesem Gerät nicht verfügbar.'));
            return;
        }

        try {
            const stream = await navigator.mediaDevices.getUserMedia({
                video: { facingMode: 'environment' },
            });
            streamRef.current = stream;
            if (videoRef.current) {
                videoRef.current.srcObject = stream;
                videoRef.current.play().catch(() => {});
            }
        } catch (err) {
            logger.info(`Camera access not available or denied: ${stringifyError(err)}`);
            setCameraError(t('contacts.cameraAccessDenied', 'Kamera konnte nicht gestartet werden. Bitte erlaube den Kamerazugriff oder nutze eine andere Methode.'));
        }
    };

    const handleImportPayload = async (rawPayload: string) => {
        const trimmed = rawPayload.trim();
        if (!trimmed) {
            onError(t('contacts.emptyClipboard'));
            return;
        }

        // 1. Check if it's a direct DID
        if (trimmed.startsWith('did:key:')) {
            onClose();
            onOpenManualAdd(trimmed);
            return;
        }

        // 2. Check if it's a humoco://contact URI
        if (trimmed.startsWith('humoco://contact') || trimmed.includes('did%3Akey%3A')) {
            const parsed = parseContactCard(trimmed);
            if (parsed && parsed.did) {
                onClose();
                onOpenManualAdd(parsed.did, parsed.name ? { firstName: parsed.name } : undefined);
                return;
            }
        }

        // 3. Otherwise try importing as a signed card (.hmcard, JSON, or card URI)
        setIsProcessing(true);
        try {
            const res = await protectAction(async (pwd) => {
                return await marketService.importContactCard(trimmed, pwd || undefined);
            });

            if (res && res.status === 'accepted') {
                await onContactAdded();
                onSuccess(
                    res.replaced
                        ? t('marketplace.importUpdatedDesc', 'Visitenkarte erfolgreich aktualisiert.')
                        : t('marketplace.importSuccessDesc', 'Visitenkarte erfolgreich importiert.')
                );
                onClose();
            } else if (res && res.status === 'rejected_expired') {
                onError(t('marketplace.importExpiredDesc', 'Diese Visitenkarte ist abgelaufen.'));
            } else if (res && res.status === 'rejected_offender') {
                onError(t('marketplace.importOffenderDesc', 'Diese Visitenkarte wurde wegen Betrugsverdacht blockiert.'));
            } else {
                // If direct card import failed, but text is a plain string, let user edit it manually
                if (trimmed.length > 20 && !trimmed.startsWith('{')) {
                    onClose();
                    onOpenManualAdd(trimmed);
                } else {
                    onError(t('contacts.invalidClipboardContent'));
                }
            }
        } catch (err) {
            logger.error(`Import card error: ${stringifyError(err)}`);
            onError(stringifyError(err));
        } finally {
            setIsProcessing(false);
        }
    };

    // Tile 2: Clipboard
    const handleTileClipboard = async () => {
        try {
            const { text, error } = await readFromClipboard();
            if (error || !text || !text.trim()) {
                // Open paste input view if direct read is empty or restricted
                setView('paste_input');
                return;
            }
            await handleImportPayload(text);
        } catch (e) {
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
                    { name: 'All Files', extensions: ['*'] },
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
            {/* View 1: 4 Tiles */}
            {view === 'tiles' && (
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
            )}

            {/* View 2: QR Scanner */}
            {view === 'qr_scanner' && (
                <div className="space-y-4 py-2" data-testid="qr-scanner-view">
                    <div className="relative w-full aspect-square max-h-[340px] bg-slate-900 rounded-2xl overflow-hidden flex items-center justify-center border border-slate-700">
                        {cameraError ? (
                            <div className="p-6 text-center space-y-3 max-w-sm">
                                <Camera size={36} className="text-slate-400 mx-auto" />
                                <p className="text-xs text-slate-300 leading-relaxed">
                                    {cameraError}
                                </p>
                                <div className="flex flex-col sm:flex-row gap-2 justify-center pt-2">
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="secondary"
                                        onClick={() => setView('paste_input')}
                                    >
                                        {t('contacts.addTileClipboardTitle', 'Link/Code einfügen')}
                                    </Button>
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="secondary"
                                        onClick={handleTileFile}
                                    >
                                        {t('contacts.addTileFileTitle', 'Datei öffnen (.hmcard)')}
                                    </Button>
                                </div>
                            </div>
                        ) : (
                            <>
                                <video
                                    ref={videoRef}
                                    className="w-full h-full object-cover"
                                    playsInline
                                    muted
                                />
                                <div className="absolute inset-0 border-2 border-white/30 rounded-2xl pointer-events-none flex items-center justify-center">
                                    <div className="w-56 h-56 border-2 border-emerald-400 rounded-2xl shadow-[0_0_0_9999px_rgba(0,0,0,0.4)]" />
                                </div>
                            </>
                        )}
                    </div>

                    <div className="flex justify-between items-center text-xs text-theme-light">
                        <button
                            type="button"
                            onClick={() => setView('tiles')}
                            className="font-bold text-theme-secondary hover:text-theme-primary transition-colors"
                        >
                            ← {t('common.back', 'Zurück zur Auswahl')}
                        </button>
                        <button
                            type="button"
                            onClick={() => setView('paste_input')}
                            className="font-bold text-theme-primary hover:underline"
                        >
                            {t('contacts.pasteLinkInstead', 'Lieber als Text einfügen')} →
                        </button>
                    </div>
                </div>
            )}

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
