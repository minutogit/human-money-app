import React, { useState, useRef, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { UploadCloud } from 'lucide-react';
import { useNavigation } from '../context/NavigationContext';
import { useSession } from '../context/SessionContext';

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
  '.hmcproof'
];


export function GlobalFileDropZone({ children }: GlobalFileDropZoneProps) {
  const { t } = useTranslation();
  const { appState, navigate } = useNavigation();
  const { isAuthenticated, isSessionActive } = useSession();
  const [isDraggingOver, setIsDraggingOver] = useState(false);
  const dragCounterRef = useRef(0);

  const isLoggedIn = isAuthenticated ?? isSessionActive;
  const isDropZoneActive = Boolean(isLoggedIn && appState.view !== 'receive_bundle');

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

      {isDraggingOver && (
        <div className="fixed inset-0 z-[150] bg-theme-primary/90 backdrop-blur-md flex flex-col items-center justify-center text-white p-8 animate-in fade-in duration-200 pointer-events-none">
          <div className="w-28 h-28 rounded-[36px] bg-white/10 border-2 border-white/30 flex items-center justify-center mb-6 shadow-2xl animate-bounce-subtle">
            <UploadCloud size={64} className="text-white" />
          </div>
          <h2 className="text-3xl font-black tracking-tight mb-2">
            {t('transfer.globalDrop.title')}
          </h2>
          <p className="text-sm font-medium text-white/80 max-w-md text-center">
            {t('transfer.globalDrop.description')}
          </p>
        </div>
      )}
    </div>
  );
}
