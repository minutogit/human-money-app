import React, { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { QrCode, Camera, Image as ImageIcon, Clipboard, AlertTriangle } from 'lucide-react';
import { ModalShell } from './ModalShell';
import { Button } from './Button';
import { readFromClipboard } from '../../utils/clipboard';
import { logger } from '../../utils/log';
import { stringifyError } from '../../utils/errorHelper';

interface BarcodeDetectorInstance {
    detect: (image: HTMLVideoElement | ImageBitmap) => Promise<Array<{ rawValue?: string }>>;
}

type BarcodeDetectorConstructor = new (options?: { formats?: string[] }) => BarcodeDetectorInstance;

export interface QrScannerModalProps {
    isOpen: boolean;
    onClose: () => void;
    onScan: (decodedText: string) => void;
    title?: string;
    hint?: string;
}

export const QrScannerModal: React.FC<QrScannerModalProps> = ({
    isOpen,
    onClose,
    onScan,
    title,
    hint,
}) => {
    const { t } = useTranslation();
    const [cameraError, setCameraError] = useState<string | null>(null);
    const [clipboardError, setClipboardError] = useState<string | null>(null);
    const [isScanningFile, setIsScanningFile] = useState(false);

    const videoRef = useRef<HTMLVideoElement | null>(null);
    const streamRef = useRef<MediaStream | null>(null);
    const scanTimerRef = useRef<number | null>(null);
    const fileInputRef = useRef<HTMLInputElement | null>(null);

    const hasBarcodeDetector = typeof window !== 'undefined' && 'BarcodeDetector' in window;

    const stopCamera = useCallback(() => {
        if (scanTimerRef.current) {
            clearTimeout(scanTimerRef.current);
            scanTimerRef.current = null;
        }
        if (streamRef.current) {
            streamRef.current.getTracks().forEach((track) => track.stop());
            streamRef.current = null;
        }
        if (videoRef.current) {
            videoRef.current.srcObject = null;
        }
    }, []);

    useEffect(() => {
        if (!isOpen) {
            stopCamera();
            setCameraError(null);
            setClipboardError(null);
            return;
        }

        if (!hasBarcodeDetector) {
            setCameraError(t('common.qrScanUnsupported'));
            return;
        }
        if (typeof navigator === 'undefined' || !navigator.mediaDevices?.getUserMedia) {
            setCameraError(t('contacts.cameraNotSupported'));
            return;
        }

        let active = true;
        let detector: BarcodeDetectorInstance | null = null;

        try {
            const Ctor = (window as unknown as { BarcodeDetector: BarcodeDetectorConstructor }).BarcodeDetector;
            detector = new Ctor({ formats: ['qr_code'] });
        } catch (err) {
            logger.info(`BarcodeDetector creation failed: ${stringifyError(err)}`);
            setCameraError(t('common.qrScanUnsupported'));
            return;
        }

        const start = async () => {
            try {
                const stream = await navigator.mediaDevices.getUserMedia({
                    video: { facingMode: 'environment' },
                });
                if (!active) {
                    stream.getTracks().forEach((track) => track.stop());
                    return;
                }
                streamRef.current = stream;
                if (videoRef.current) {
                    videoRef.current.srcObject = stream;
                    videoRef.current.play().catch(() => {});
                }

                const scanLoop = async () => {
                    if (!active) return;
                    if (detector && videoRef.current && videoRef.current.readyState >= 2) {
                        try {
                            const codes = await detector.detect(videoRef.current);
                            const raw = codes?.[0]?.rawValue?.trim();
                            if (raw) {
                                active = false;
                                stopCamera();
                                onScan(raw);
                                onClose();
                                return;
                            }
                        } catch {
                            // Continue frame polling
                        }
                    }
                    if (active) {
                        scanTimerRef.current = window.setTimeout(scanLoop, 250);
                    }
                };
                scanTimerRef.current = window.setTimeout(scanLoop, 350);
            } catch (err) {
                if (!active) return;
                logger.info(`Camera access denied or unavailable: ${stringifyError(err)}`);
                setCameraError(t('contacts.cameraAccessDenied'));
            }
        };

        void start();

        return () => {
            active = false;
            stopCamera();
        };
    }, [isOpen, hasBarcodeDetector, stopCamera, onScan, onClose, t]);

    const handleImageFileChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
        const file = e.target.files?.[0];
        e.target.value = '';
        if (!file) return;

        setCameraError(null);
        setClipboardError(null);

        if (!hasBarcodeDetector || typeof createImageBitmap !== 'function') {
            setCameraError(t('common.qrScanUnsupported'));
            return;
        }

        setIsScanningFile(true);
        try {
            const bitmap = await createImageBitmap(file);
            try {
                const Ctor = (window as unknown as { BarcodeDetector: BarcodeDetectorConstructor }).BarcodeDetector;
                const detector = new Ctor({ formats: ['qr_code'] });
                const codes = await detector.detect(bitmap);
                const raw = codes?.[0]?.rawValue?.trim();
                if (raw) {
                    stopCamera();
                    onScan(raw);
                    onClose();
                    return;
                }
                setCameraError(t('common.qrImageNoCodeFound'));
            } finally {
                bitmap.close();
            }
        } catch (err) {
            logger.info(`QR image scan failed: ${stringifyError(err)}`);
            setCameraError(t('common.qrImageNoCodeFound'));
        } finally {
            setIsScanningFile(false);
        }
    };

    const handlePasteFallback = async () => {
        setClipboardError(null);
        try {
            const { text, error } = await readFromClipboard();
            if (error || !text || !text.trim()) {
                setClipboardError(t('common.qrClipboardEmpty'));
                return;
            }
            stopCamera();
            onScan(text.trim());
            onClose();
        } catch (err) {
            setClipboardError(stringifyError(err));
        }
    };

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={onClose}
            maxWidth="md"
            title={
                <div className="flex items-center gap-2.5">
                    <div className="w-9 h-9 rounded-xl bg-theme-primary/10 text-theme-primary flex items-center justify-center">
                        <QrCode size={18} />
                    </div>
                    <div>
                        <h3 className="text-base font-bold text-theme-secondary">
                            {title || t('common.scanQrCode')}
                        </h3>
                        {hint && <p className="text-xs text-theme-light">{hint}</p>}
                    </div>
                </div>
            }
            footer={
                <div className="flex items-center justify-end w-full">
                    <Button type="button" variant="secondary" size="sm" onClick={onClose}>
                        {t('common.close')}
                    </Button>
                </div>
            }
        >
            <div data-testid="qr-scanner-view" className="space-y-4">
                {cameraError ? (
                    <div
                        data-testid="qr-scanner-camera-error"
                        className="p-4 rounded-2xl border border-amber-200 bg-amber-50/80 text-amber-900 text-xs flex items-start gap-2.5"
                    >
                        <AlertTriangle size={16} className="text-amber-600 shrink-0 mt-0.5" />
                        <div className="space-y-1">
                            <p className="font-bold">{cameraError}</p>
                            <p className="text-[11px] opacity-85">{t('common.qrFallbackHint')}</p>
                        </div>
                    </div>
                ) : (
                    <div className="relative rounded-2xl overflow-hidden bg-slate-950 aspect-video flex items-center justify-center border border-theme-subtle">
                        <video
                            ref={videoRef}
                            playsInline
                            muted
                            className="w-full h-full object-cover"
                        />
                        <div className="pointer-events-none absolute inset-6 border-2 border-white/50 rounded-2xl" />
                        <div className="absolute bottom-2.5 left-1/2 -translate-x-1/2 px-3 py-1 rounded-full bg-black/60 text-white text-[11px] font-medium flex items-center gap-1.5">
                            <Camera size={12} />
                            <span>{t('common.qrPointCameraHint')}</span>
                        </div>
                    </div>
                )}

                {clipboardError && (
                    <p className="text-xs font-semibold text-rose-600">{clipboardError}</p>
                )}

                <div className="flex flex-wrap gap-2">
                    <input
                        ref={fileInputRef}
                        type="file"
                        accept="image/*"
                        onChange={handleImageFileChange}
                        className="hidden"
                    />
                    {hasBarcodeDetector && (
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={isScanningFile}
                            onClick={() => fileInputRef.current?.click()}
                            data-testid="qr-scanner-image-btn"
                            className="flex-1 gap-1.5 text-xs"
                        >
                            <ImageIcon size={14} />
                            <span>{t('common.qrChooseImageBtn')}</span>
                        </Button>
                    )}
                    <Button
                        type="button"
                        variant={cameraError ? 'primary' : 'outline'}
                        size="sm"
                        onClick={handlePasteFallback}
                        data-testid="qr-scanner-paste-btn"
                        className="flex-1 gap-1.5 text-xs"
                    >
                        <Clipboard size={14} />
                        <span>{t('common.qrPasteClipboardBtn')}</span>
                    </Button>
                </div>
            </div>
        </ModalShell>
    );
};
