import React, { useState, useRef, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { UploadCloud, CheckCircle2, AlertCircle, Info, X } from 'lucide-react';
import { useNavigation } from '../../context/NavigationContext';
import { useSession } from '../../context/SessionContext';
import { marketService } from '../../services/marketService';
import { backupService } from '../../services/backupService';
import { logger } from '../../utils/log';
import { stringifyError, translateError } from '../../utils/errorHelper';
import {
  classifyExtension,
  classifyHumocoFile,
  extractHumocoDeepLink,
} from '../../utils/humocoFileClassifier';

interface GlobalFileDropZoneProps {
  children: React.ReactNode;
}

export function GlobalFileDropZone({ children }: GlobalFileDropZoneProps) {
  const { t } = useTranslation();
  const { appState, navigate, setPendingDroppedFile } = useNavigation();
  const { isAuthenticated, protectAction } = useSession();
  const [isDraggingOver, setIsDraggingOver] = useState(false);
  const dragCounterRef = useRef(0);

  const [dropFeedback, setDropFeedback] = useState<{
    status: string;
    message: string;
    did?: string;
  } | null>(null);

  const isLoggedIn = isAuthenticated;
  const isDropZoneActive = Boolean(appState.view !== 'receive_bundle');

  // Auto-dismiss drop feedback after 5 seconds
  useEffect(() => {
    if (dropFeedback) {
      const timer = setTimeout(() => setDropFeedback(null), 5000);
      return () => clearTimeout(timer);
    }
  }, [dropFeedback]);

  // Reset drag state if active status changes or navigation occurs
  useEffect(() => {
    dragCounterRef.current = 0;
    setIsDraggingOver(false);
  }, [appState.view, isDropZoneActive]);

  const handleDragEnter = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (!isDropZoneActive) return;

    dragCounterRef.current += 1;
    const types = e.dataTransfer?.types;
    if (types?.includes('Files') || types?.includes('text/plain') || types?.includes('text/uri-list')) {
      setIsDraggingOver(true);
    }
  };

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
  };

  const handleDragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (!isDropZoneActive) return;

    dragCounterRef.current -= 1;
    if (dragCounterRef.current <= 0) {
      dragCounterRef.current = 0;
      setIsDraggingOver(false);
    }
  };

  const stageAndRestoreBackup = async (bytes: number[]) => {
    try {
      const stagedPath = await backupService.stageDroppedBackupBytes(bytes);
      navigate({
        view: 'restore_backup',
        initialMethod: 'file_or_folder',
        initialBackupPath: stagedPath,
      });
    } catch (err) {
      logger.error('Failed to stage dropped .hmcbackup: ' + stringifyError(err));
      setDropFeedback({
        status: 'error',
        message: translateError(err, t),
      });
    }
  };

  const importDroppedCard = async (text: string) => {
    try {
      const res = await protectAction(async (pwd) => {
        return await marketService.importContactCard(text, pwd || undefined);
      });
      if (!res) return;

      const statusMessages: Record<string, string> = {
        accepted: res.replaced ? t('marketplace.importUpdatedDesc') : t('marketplace.importSuccessDesc'),
        ignored_stale: t('marketplace.importIgnoredDesc'),
        rejected_expired: t('marketplace.importExpiredDesc'),
        rejected_hops: t('marketplace.importHopsDesc'),
        rejected_offender: t('marketplace.importOffenderDesc'),
      };

      setDropFeedback({
        status: res.status,
        message: statusMessages[res.status] ?? res.status,
        did: res.did ?? undefined,
      });

      if (res.status === 'accepted' && res.did) {
        navigate({
          view: 'marketplace',
          selectedDid: res.did,
        });
      }
    } catch (err) {
      logger.error('Failed to import dropped .hmcard: ' + stringifyError(err));
      setDropFeedback({
        status: 'error',
        message: translateError(err, t),
      });
    }
  };

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragCounterRef.current = 0;
    setIsDraggingOver(false);
    if (!isDropZoneActive) return;

    const rawText = e.dataTransfer?.getData('text/plain') || e.dataTransfer?.getData('text/uri-list');
    const droppedDeepLink = extractHumocoDeepLink(rawText);
    if (droppedDeepLink) {
      window.dispatchEvent(new CustomEvent('humoco:deeplink', { detail: droppedDeepLink }));
      return;
    }

    const files = e.dataTransfer?.files;
    if (!files || files.length === 0) return;

    const file = files[0];
    let bytes: number[];
    try {
      const buf = await file.arrayBuffer();
      bytes = Array.from(new Uint8Array(buf));
    } catch (err) {
      logger.error('Failed to read dropped file globally: ' + stringifyError(err));
      setDropFeedback({ status: 'error', message: translateError(err, t) });
      return;
    }

    if (classifyExtension(file.name) === 'backup') {
      await stageAndRestoreBackup(bytes);
      return;
    }

    const classification = classifyHumocoFile({ fileName: file.name, bytes });

    if (classification.kind === 'deeplink' && classification.deepLink) {
      window.dispatchEvent(new CustomEvent('humoco:deeplink', { detail: classification.deepLink }));
      return;
    }

    if (classification.kind === 'backup') {
      await stageAndRestoreBackup(bytes);
      return;
    }

    if (classification.kind === 'unknown') {
      setDropFeedback({
        status: 'error',
        message: t('common.unsupportedFileDrop'),
      });
      return;
    }

    if (!isLoggedIn) {
      setPendingDroppedFile({ name: file.name, bytes });
      setDropFeedback({
        status: 'info',
        message: t('auth.pendingFileNotice', 'Datei vorgemerkt – wird direkt nach dem Anmelden geöffnet.'),
      });
      return;
    }

    if (classification.kind === 'card') {
      const text = new TextDecoder().decode(new Uint8Array(bytes));
      await importDroppedCard(text);
      return;
    }

    navigate({
      view: 'receive_bundle',
      droppedFile: {
        name: file.name,
        bytes,
      },
    });
  };

  return (
    <div
      className="relative w-full h-full flex flex-1 flex-col overflow-hidden"
      onDragEnter={handleDragEnter}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {children}

      {/* Drop Feedback Toast */}
      {dropFeedback && (
        <div className="fixed top-4 right-4 z-[9999] max-w-sm p-4 bg-white dark:bg-gray-900 border border-theme-subtle rounded-2xl shadow-xl space-y-1 animate-in fade-in slide-in-from-top-2">
          <div className="flex items-center justify-between gap-3">
            <div className="flex items-center gap-2">
              {dropFeedback.status === 'accepted' ? (
                <CheckCircle2 size={16} className="text-emerald-600" />
              ) : dropFeedback.status === 'ignored_stale' || dropFeedback.status === 'info' ? (
                <Info size={16} className="text-amber-600" />
              ) : (
                <AlertCircle size={16} className="text-rose-600" />
              )}
              <span className="text-xs font-bold text-theme-secondary">
                {dropFeedback.status === 'accepted'
                  ? t('marketplace.importSuccessTitle')
                  : dropFeedback.status === 'info'
                    ? t('common.info', 'Hinweis')
                    : t('marketplace.importStatusTitle')}
              </span>
            </div>
            <button
              onClick={() => setDropFeedback(null)}
              className="text-theme-light hover:text-theme-secondary p-0.5 cursor-pointer"
            >
              <X size={14} />
            </button>
          </div>
          <p className="text-xs text-theme-light leading-relaxed pl-6">{dropFeedback.message}</p>
        </div>
      )}

      {/* Global Drag Overlay */}
      {isDraggingOver && (
        <div className="absolute inset-0 z-50 flex flex-col items-center justify-center bg-theme-primary/10 backdrop-blur-xs border-4 border-dashed border-theme-primary transition-all duration-200 pointer-events-none">
          <div className="bg-white dark:bg-gray-900 px-8 py-6 rounded-3xl shadow-xl flex flex-col items-center max-w-sm text-center">
            <div className="w-16 h-16 rounded-full bg-theme-primary/10 flex items-center justify-center text-theme-primary mb-4 animate-bounce">
              <UploadCloud size={32} />
            </div>
            <h3 className="text-lg font-bold text-theme-secondary mb-1">
              {t('transfer.globalDrop.title')}
            </h3>
            <p className="text-xs text-theme-light">
              {t('transfer.globalDrop.description')}
            </p>
          </div>
        </div>
      )}
    </div>
  );
}
