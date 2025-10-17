// src/components/ReceiveView.tsx
import { useState, useEffect, FormEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { readFile } from '@tauri-apps/plugin-fs';
import { logger } from '../utils/log';
import { VoucherStandardInfo, ReceiveSuccessPayload } from '../types';
import { Button } from './ui/Button';
import { Input } from './ui/Input';

interface ReceiveViewProps {
    onBack: () => void;
    onReceiveSuccess: (payload: ReceiveSuccessPayload) => void;
}

export function ReceiveView({ onBack, onReceiveSuccess }: ReceiveViewProps) {
    const [bundlePath, setBundlePath] = useState<string | null>(null);
    const [bundleName, setBundleName] = useState<string | null>(null);
    const [password, setPassword] = useState('');
    const [voucherStandards, setVoucherStandards] = useState<VoucherStandardInfo[]>([]);
    const [feedbackMsg, setFeedbackMsg] = useState('');
    const [isProcessing, setIsProcessing] = useState(false);

    useEffect(() => {
        async function fetchStandards() {
            try {
                logger.info("ReceiveView opened, fetching standards.");
                const standards = await invoke<VoucherStandardInfo[]>("get_voucher_standards");
                setVoucherStandards(standards);
            } catch (e) {
                const msg = `Failed to fetch voucher standards: ${e}`;
                logger.error(msg);
                setFeedbackMsg(`Error: ${msg}`);
            }
        }
        fetchStandards();
    }, []);

    const handleFileSelect = async () => {
        setFeedbackMsg('');
        try {
            const selectedPath = await open({
                multiple: false,
                filters: [{ name: 'Minuto Bundle', extensions: ['minuto-bundle'] }]
            });

            if (typeof selectedPath === 'string') {
                logger.info(`File selected via dialog: ${selectedPath}`);
                setBundlePath(selectedPath);
                setBundleName(selectedPath.split(/[/\\]/).pop() || 'bundle.minuto-bundle');
            }
        } catch (e) {
            const msg = `Error selecting file: ${e}`;
            logger.error(msg);
            setFeedbackMsg(msg);
        }
    };



    const clearSelection = () => {
        setBundlePath(null);
        setBundleName(null);
    }

    // Drag and drop functionality
    const [isDragOver, setIsDragOver] = useState(false);
    const [droppedFileContent, setDroppedFileContent] = useState<number[] | null>(null);

    // Prevent default browser behavior for drag events to enable drop functionality
    useEffect(() => {
        // Add a more comprehensive drag event handler
        const preventGlobalDefaults = (e: Event) => {
            e.preventDefault();
            // Don't stop propagation so events can reach our component
        };

        const handleGlobalDrop = (e: Event) => {
            // Still need to prevent default to prevent browser default behavior
            e.preventDefault();
        };

        // Add capture-phase listeners to catch events early
        document.addEventListener("dragenter", preventGlobalDefaults, true);
        document.addEventListener("dragover", preventGlobalDefaults, true);
        document.addEventListener("dragleave", preventGlobalDefaults, true);
        document.addEventListener("drop", handleGlobalDrop, true);

        // Also add bubble-phase listeners
        document.addEventListener("dragenter", preventGlobalDefaults, false);
        document.addEventListener("dragover", preventGlobalDefaults, false);
        document.addEventListener("dragleave", preventGlobalDefaults, false);
        document.addEventListener("drop", handleGlobalDrop, false);

        // Cleanup
        return () => {
            document.removeEventListener("dragenter", preventGlobalDefaults, true);
            document.removeEventListener("dragover", preventGlobalDefaults, true);
            document.removeEventListener("dragleave", preventGlobalDefaults, true);
            document.removeEventListener("drop", handleGlobalDrop, true);

            document.removeEventListener("dragenter", preventGlobalDefaults, false);
            document.removeEventListener("dragover", preventGlobalDefaults, false);
            document.removeEventListener("dragleave", preventGlobalDefaults, false);
            document.removeEventListener("drop", handleGlobalDrop, false);
        };
    }, []);

    const handleDragOver = (e: React.DragEvent<HTMLDivElement>) => {
        e.preventDefault();
        e.stopPropagation();
        if (!isDragOver) {
            setIsDragOver(true);
        }
    };

    const handleDragLeave = (e: React.DragEvent<HTMLDivElement>) => {
        e.preventDefault();
        e.stopPropagation();
        // Only reset if we're actually leaving the element, not just moving between child elements
        setTimeout(() => {
            if (!e.currentTarget.contains(e.relatedTarget as Node)) {
                setIsDragOver(false);
            }
        }, 0);
    };

    const handleDrop = async (e: React.DragEvent<HTMLDivElement>) => {
        e.preventDefault();
        e.stopPropagation();
        setIsDragOver(false);

        // Log data transfer info

        const files = e.dataTransfer.files;
        
        if (files.length > 0) {
            const file = files[0];
            logger.info(`Processing dropped file: ${file.name} (Size: ${file.size})`);
            
            // Check if the file is a .minuto-bundle file
            if (file.name.endsWith('.minuto-bundle')) {
                try {
                    // Read the file content as ArrayBuffer
                    const fileBuffer = await file.arrayBuffer();
                    const uint8Array = new Uint8Array(fileBuffer);
                    
                    // Convert to number array for compatibility with our backend
                    const bundleData = Array.from(uint8Array);
                    logger.info(`Bundle data length: ${bundleData.length}`);
                    
                    // Set the file info and content
                    setBundlePath(file.name); // Using filename as path indicator
                    setBundleName(file.name);
                    setDroppedFileContent(bundleData);
                    logger.info(`Successfully set dropped file: ${file.name}`);
                } catch (error) {
                    const msg = `Error processing dropped file: ${error}`;
                    logger.error(msg);
                    setFeedbackMsg(msg);
                }
            } else {
                const msg = `Invalid file type. Please drop a '.minuto-bundle' file. Got: ${file.name}`;
                logger.warn(msg);
                setFeedbackMsg(msg);
            }
        } else {
            const msg = "No files found in drop event. This might be due to security restrictions or an unsupported file type.";
            logger.info(msg);
            setFeedbackMsg(msg);
        }
    };

    // Modify handleProcessBundle to handle dropped files
    const handleProcessBundle = async (event: FormEvent) => {
        event.preventDefault();
        logger.info(`Attempting to process bundle...`);
        
        if ((!bundlePath || !password) && !droppedFileContent) {
            setFeedbackMsg("Please select a bundle file and enter your password.");
            return;
        }

        setIsProcessing(true);
        setFeedbackMsg('');

        try {
            let bundleData: number[];
            if (droppedFileContent) {
                // Use the dropped file content
                bundleData = droppedFileContent;
            } else if (bundlePath) {
                // Read file from path (original logic)
                const bundleUint8Array = await readFile(bundlePath);
                bundleData = Array.from(bundleUint8Array);
            } else {
                throw new Error("No bundle file provided");
            }

            // KORREKTUR: Das Backend benötigt eine Map, die von der Standard-UUID (aus dem TOML-Inhalt)
            // auf den TOML-Inhalt abbildet, nicht vom Ordnernamen.
            const standardDefinitionsToml: Record<string, string> = {};
            voucherStandards.forEach(standard => {
                const uuidMatch = standard.content.match(/uuid\s*=\s*"([^"]+)"/);
                if (uuidMatch && uuidMatch[1]) {
                    const uuid = uuidMatch[1];
                    standardDefinitionsToml[uuid] = standard.content;
                }
            });

            logger.info(`Calling receive_bundle with ${bundleData.length} bytes of data and ${Object.keys(standardDefinitionsToml).length} standards`);
            
            const payload = await invoke<ReceiveSuccessPayload>("receive_bundle", {
                bundleData,
                standardDefinitionsToml,
                password
            });

            logger.info("Bundle received and processed successfully.");
            onReceiveSuccess(payload);

        } catch (e) {
            // Verbessertes Logging für detailliertere Fehler
            const msg = `Failed during bundle processing. Error: ${e instanceof Error ? e.message : String(e)}`;
            logger.error(msg);
            if (e instanceof Error && e.stack) logger.error(e.stack);
            setFeedbackMsg(`Error: ${msg}`);
        } finally {
            setIsProcessing(false);
        }
    };

    return (
        <div className="flex flex-col h-full max-w-2xl mx-auto">
            <header className="flex-shrink-0 mb-6">
                <div className="flex items-center justify-between">
                    <h1 className="text-2xl font-bold text-theme-primary">Receive Funds</h1>
                    <Button variant="secondary" onClick={onBack}>Cancel</Button>
                </div>
                <p className="text-theme-light mt-1">Process a transfer bundle you have received.</p>
            </header>

            <div className="flex-grow">
                <form onSubmit={handleProcessBundle} className="space-y-6">
                    {feedbackMsg && <p className="text-center text-red-500">{feedbackMsg}</p>}

                    <div
                        id="bundle-drop-zone"
                        className={`border-2 border-dashed rounded-lg p-8 text-center cursor-pointer ${
                            isDragOver ? 'border-theme-primary bg-bg-input-focus' : 'border-theme-subtle'
                        }`}
                        onDragOver={handleDragOver}
                        onDragLeave={handleDragLeave}
                        onDrop={handleDrop}
                        style={{ minHeight: '200px', width: '100%' }}
                    >
                        {bundleName ? (
                            <div>
                                <p className="font-semibold text-theme-primary">{bundleName}</p>
                                <p className="text-sm text-theme-light mt-2">File is ready to be processed.</p>
                                <Button type="button" variant="outline" onClick={clearSelection}>Choose a different file</Button>
                            </div>
                        ) : (
                            <div>
                                <p className="text-theme-light mb-4">Drag & Drop your '.minuto-bundle' file here</p>
                                <p className="text-theme-light text-sm mb-4">or</p>
                                <Button type="button" onClick={handleFileSelect}>Select Bundle File</Button>
                            </div>
                        )}
                    </div>

                    {(bundlePath || droppedFileContent) && (
                        <div className="space-y-4 bg-bg-card border border-theme-subtle rounded-lg p-4">
                            <div>
                                <label htmlFor="password" className="block text-sm font-medium text-theme-light mb-1">Your Wallet Password</label>
                                <Input
                                    id="password"
                                    type="password"
                                    value={password}
                                    onChange={(e) => setPassword(e.target.value)}
                                    required
                                    autoComplete="current-password"
                                    placeholder="Enter password to confirm"
                                    disabled={isProcessing}
                                />
                            </div>
                            <Button size="lg" type="submit" className="w-full" disabled={(!bundlePath && !droppedFileContent) || !password || isProcessing}>
                                {isProcessing ? "Processing..." : "Process Bundle"}
                            </Button>
                        </div>
                    )}
                </form>
            </div>
        </div>
    );
}