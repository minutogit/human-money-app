import React, { useState, useRef, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { UploadCloud, CheckCircle2, AlertCircle, Info, X } from 'lucide-react';
import { useNavigation } from '../context/NavigationContext';
import { useSession } from '../context/SessionContext';
import { marketService } from '../services/marketService';
import { logger } from '../utils/log';
import { stringifyError } from '../utils/errorHelper';

interface GlobalFileDropZoneProps {
  children: React.ReactNode;
}

const VALID_DROP_EXTENSIONS = [
  '.transfer',
  '.ask',
  '.sig',
  '.wot',
  '.standard',
  '.toml',
  '.humocoreq',
  '.humocosig',
  '.hmcproof',
  '.hmcard'
];

export function GlobalFileDropZone({ children }: GlobalFileDropZoneProps) {
  const { t } = useTranslation();
  const { appState, navigate } = useNavigation();
  const { isAuthenticated, isSessionActive, protectAction } = useSession();
  const [isDraggingOver, setIsDraggingOver] = useState(false);
  const dragCounterRef = useRef(0);

  const [dropFeedback, setDropFeedback] = useState<{
    status: string;
    message: string;
    did?: string;
  } | null>(null);

  const isLoggedIn = isAuthenticated ?? isSessionActive;
  const isDropZoneActive = Boolean(isLoggedIn && appState.view !== 'receive_bundle');

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
    if (e.dataTransfer?.types?.includes('Files')) {
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

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragCounterRef.current = 0;
    setIsDraggingOver(false);
    if (!isDropZoneActive) return;

    const files = e.dataTransfer?.files;
    if (files && files.length > 0) {
      const file = files[0];

      // Handle .hmcard files specifically
      if (file.name.endsWith('.hmcard')) {
        try {
          const text = await file.text();
          const res = await protectAction(async (pwd) => {
            return await marketService.importContactCard(text, pwd || undefined);
          });
          if (res) {
            let feedback = '';
            if (res.status === 'accepted') {
              feedback = res.replaced ? t('marketplace.importUpdatedDesc') : t('marketplace.importSuccessDesc');
            } else if (res.status === 'ignored_stale') {
              feedback = t('marketplace.importIgnoredDesc');
            } else if (res.status === 'rejected_expired') {
              feedback = t('marketplace.importExpiredDesc');
            } else if (res.status === 'rejected_hops') {
              feedback = t('marketplace.importHopsDesc');
            } else if (res.status === 'rejected_offender') {
              feedback = t('marketplace.importOffenderDesc');
            } else {
              feedback = res.status;
            }
            setDropFeedback({
              status: res.status,
              message: feedback,
              did: res.did ?? undefined,
            });
            navigate({
              view: 'marketplace',
              selectedDid: res.did ?? undefined,
            });
          }
        } catch (err) {
          logger.error("Failed to import dropped .hmcard: " + stringifyError(err));
          setDropFeedback({
            status: 'error',
            message: stringifyError(err),
          });
        }
        return;
      }

      const hasValidExt = VALID_DROP_EXTENSIONS.some(ext => file.name.endsWith(ext));
      if (hasValidExt) {
        try {
          const buf = await file.arrayBuffer();
          const bytes = Array.from(new Uint8Array(buf));
          navigate({
            view: 'receive_bundle',
            droppedFile: {
              name: file.name,
              bytes,
            },
          });
        } catch (err) {
          console.error("Failed to read dropped file globally:", err);
        }
      }
    } else {
      // Check for dropped plain text links (humoco://card?env=...)
      const rawText = e.dataTransfer?.getData('text/plain');
      if (rawText && (rawText.includes('humoco://card') || rawText.includes('?env='))) {
        try {
          const res = await protectAction(async (pwd) => {
            return await marketService.importContactCard(rawText, pwd || undefined);
          });
          if (res) {
            navigate({
              view: 'marketplace',
              selectedDid: res.did ?? undefined,
            });
          }
        } catch (err) {
          logger.error("Failed to import card from text drop: " + stringifyError(err));
        }
      }
    }
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
        <div className="fixed top-4 right-4 z-[9999] max-w-sm p-4 bg-white border border-theme-subtle rounded-2xl shadow-xl space-y-1 animate-in fade-in slide-in-from-top-2">
          <div className="flex items-center justify-between gap-3">
            <div className="flex items-center gap-2">
              {dropFeedback.status === 'accepted' ? (
                <CheckCircle2 size={16} className="text-emerald-600" />
              ) : dropFeedback.status === 'ignored_stale' ? (
                <Info size={16} className="text-amber-600" />
              ) : (
                <AlertCircle size={16} className="text-rose-600" />
              )}
              <span className="text-xs font-bold text-theme-secondary">
                {dropFeedback.status === 'accepted' ? t('marketplace.importSuccessTitle') : t('marketplace.importStatusTitle')}
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
          <div className="bg-white px-8 py-6 rounded-3xl shadow-xl flex flex-col items-center max-w-sm text-center">
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
