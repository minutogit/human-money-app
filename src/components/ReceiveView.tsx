// src/components/ReceiveView.tsx
import { useState, useEffect, FormEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { readFile } from '@tauri-apps/plugin-fs';
import { logger } from '../utils/log';
import { Button } from './ui/Button';
import { ConfirmationModal } from './ui/ConfirmationModal';
import { updateLastUsedDirectory } from '../utils/settingsUtils';
import { useSession } from '../context/SessionContext';
import { AppSettings, VoucherStandardInfo, ReceiveSuccessPayload, VoucherDetails } from '../types';

interface ReceiveViewProps {
    onBack: () => void;
    onReceiveSuccess: (payload: ReceiveSuccessPayload) => void;
}

export function ReceiveView({ onBack, onReceiveSuccess }: ReceiveViewProps) {
    const { protectAction } = useSession();
    const [bundlePath, setBundlePath] = useState<string | null>(null);
    const [bundleName, setBundleName] = useState<string | null>(null);
    const [voucherStandards, setVoucherStandards] = useState<VoucherStandardInfo[]>([]);
    const [feedbackMsg, setFeedbackMsg] = useState('');
    const [showConfirm, setShowConfirm] = useState(false);
    const [isProcessing, setIsProcessing] = useState(false);
    const [fileType, setFileType] = useState<'transfer' | 'ask' | 'sig' | null>(null);
    const [bundlePassword, setBundlePassword] = useState("");
    const [settings, setSettings] = useState<AppSettings | null>(null);
    const [resultModal, setResultModal] = useState<{
        isOpen: boolean;
        title: string;
        content: React.ReactNode;
        confirmText: string;
        voucherId?: string;
    } | null>(null);

    useEffect(() => {
        async function fetchData() {
            try {
                logger.info("ReceiveView opened, fetching initial data.");
                const [standards, currentSettings] = await Promise.all([
                    invoke<VoucherStandardInfo[]>("get_voucher_standards"),
                    invoke<AppSettings>('get_app_settings').catch(e => {
                        logger.warn(`Failed to fetch app settings: ${e}`);
                        return null;
                    })
                ]);
                setVoucherStandards(standards);
                setSettings(currentSettings);
            } catch (e) {
                const msg = `Failed to fetch initial data: ${e}`;
                logger.error(msg);
                setFeedbackMsg(`Error: ${msg}`);
            }
        }
        fetchData();
    }, []);

    const handleFileSelect = async () => {
        setFeedbackMsg('');
        try {
            const selectedPath = await open({
                multiple: false,
                defaultPath: settings?.last_used_directory,
                filters: [
                    { name: 'All Human Money Files', extensions: ['transfer', 'ask', 'sig', 'humocoreq', 'humocosig'] },
                    { name: 'Transfer Bundle (.transfer)', extensions: ['transfer'] },
                    { name: 'Signature Request (.ask)', extensions: ['ask', 'humocoreq'] },
                    { name: 'Signature Response (.sig)', extensions: ['sig', 'humocosig'] },
                    { name: 'All Files', extensions: ['*'] }
                ]
            });

            if (typeof selectedPath === 'string') {
                logger.info(`File selected via dialog: ${selectedPath}`);
                setBundlePath(selectedPath);
                
                // Save directory for next time
                if (settings) {
                    updateLastUsedDirectory(selectedPath, settings, protectAction).then(() => {
                        // Optionally refresh settings if update was successful
                        invoke<AppSettings>('get_app_settings').then(setSettings).catch(() => {});
                    });
                }

                const fileName = selectedPath.split(/[/\\]/).pop() || '';
                setBundleName(fileName);
                
                if (fileName.endsWith('.transfer')) {
                    setFileType('transfer');
                } else if (fileName.endsWith('.ask') || fileName.endsWith('.humocoreq')) {
                    setFileType('ask');
                } else if (fileName.endsWith('.sig') || fileName.endsWith('.humocosig')) {
                    setFileType('sig');
                }
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
        setFileType(null);
        setDroppedFileContent(null);
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
            
            const validExtensions = ['.transfer', '.ask', '.sig', '.humocoreq', '.humocosig'];
            if (validExtensions.some(ext => file.name.endsWith(ext))) {
                try {
                    const fileBuffer = await file.arrayBuffer();
                    const uint8Array = new Uint8Array(fileBuffer);
                    const bundleData = Array.from(uint8Array);
                    logger.info(`File data length: ${bundleData.length}`);
                    
                    setBundlePath(file.name);
                    setBundleName(file.name);
                    setDroppedFileContent(bundleData);
                    
                    if (file.name.endsWith('.transfer')) {
                        setFileType('transfer');
                    } else if (file.name.endsWith('.ask') || file.name.endsWith('.humocoreq')) {
                        setFileType('ask');
                    } else if (file.name.endsWith('.sig') || file.name.endsWith('.humocosig')) {
                        setFileType('sig');
                    }
                    
                    logger.info(`Successfully set dropped file: ${file.name}`);
                } catch (error) {
                    const msg = `Error processing dropped file: ${error}`;
                    logger.error(msg);
                    setFeedbackMsg(msg);
                }
            } else {
                const msg = `Invalid file type. Please drop a '.transfer', '.ask', or '.sig' file. Got: ${file.name}`;
                logger.warn(msg);
                setFeedbackMsg(msg);
            }
        } else {
            const msg = "No files found in drop event. This might be due to security restrictions or an unsupported file type.";
            logger.info(msg);
            setFeedbackMsg(msg);
        }
    };

    const handleProcessClick = (event: FormEvent) => {
        event.preventDefault();
        
        if ((!bundlePath) && !droppedFileContent) {
            setFeedbackMsg("Please select a file.");
            return;
        }

        if (!fileType) {
            setFeedbackMsg("Unable to determine file type.");
            return;
        }

        setFeedbackMsg('');
        setShowConfirm(true);
    };

    async function executeReceive() {
        setIsProcessing(true);
        logger.info(`Attempting to process file of type: ${fileType}`);
        try {
            let fileData: number[];
            if (droppedFileContent) {
                fileData = droppedFileContent;
            } else if (bundlePath) {
                const fileUint8Array = await readFile(bundlePath);
                fileData = Array.from(fileUint8Array);
            } else {
                setFeedbackMsg("No file provided");
                setIsProcessing(false);
                setShowConfirm(false);
                return;
            }

            if (fileType === 'transfer') {
                const standardDefinitionsToml: Record<string, string> = {};
                voucherStandards.forEach(standard => {
                    const uuidMatch = standard.content.match(/uuid\s*=\s*"([^"]+)"/);
                    if (uuidMatch && uuidMatch[1]) {
                        const uuid = uuidMatch[1];
                        standardDefinitionsToml[uuid] = standard.content;
                    }
                });

                logger.info(`Calling receive_bundle with ${fileData.length} bytes`);

                const payload = await protectAction(async (password) => {
                    return await invoke<ReceiveSuccessPayload>("receive_bundle", {
                        bundleData: fileData,
                        standardDefinitionsToml,
                        password
                    });
                });

                if (!payload) return;
                logger.info("Bundle received and processed successfully.");
                onReceiveSuccess(payload);
            } else if (fileType === 'ask') {
                const voucher = await invoke<VoucherDetails>("open_voucher_signing_request", {
                    containerBytes: fileData,
                    password: bundlePassword || null
                });
                logger.info("Signature request opened successfully.");
                (onReceiveSuccess as any)({
                    senderId: voucher.creator.id,
                    transferSummary: { summableAmounts: {}, countableItems: {} },
                    involvedVouchers: [],
                    voucherData: voucher
                });
            } else if (fileType === 'sig') {
                const standardDefinitionsToml: Record<string, string> = {};
                voucherStandards.forEach(standard => {
                    const uuidMatch = standard.content.match(/uuid\s*=\s*"([^"]+)"/);
                    if (uuidMatch && uuidMatch[1]) {
                        const uuid = uuidMatch[1];
                        standardDefinitionsToml[uuid] = standard.content;
                    }
                });
                
                const standardTomlContent = Object.values(standardDefinitionsToml)[0];
                if (!standardTomlContent) {
                    setFeedbackMsg("No voucher standards available");
                    setIsProcessing(false);
                    setShowConfirm(false);
                    return;
                }

                const updatedInstanceId = await protectAction(async (password) => {
                    return await invoke<string>("process_and_attach_signature", {
                        containerBytes: fileData,
                        standardTomlContent,
                        containerPassword: bundlePassword || null,
                        walletPassword: password
                    });
                });
                
                if (updatedInstanceId) {
                    logger.info(`Signature processed and attached successfully to voucher: ${updatedInstanceId}`);
                    setResultModal({
                        isOpen: true,
                        title: "Signature Attached",
                        content: (
                            <div>
                                <p className="mb-2">The signature has been successfully attached to the voucher.</p>
                                <p className="text-theme-subtle">You will now be redirected to the voucher details.</p>
                            </div>
                        ),
                        confirmText: "Go to Voucher",
                        voucherId: updatedInstanceId
                    });
                }
            }

        } catch (e) {
            const errorStr = e instanceof Error ? e.message : String(e);
            
            // Check for "already attached" case
            if (errorStr.includes("already attached to voucher")) {
                // Extract local ID from our new [LOCAL_ID:...] format
                const match = errorStr.match(/\[LOCAL_ID:([\w-]+)]/);
                const voucherId = match ? match[1] : null;
                
                if (voucherId) {
                    logger.info(`Detected duplicate signature for voucher: ${voucherId}. Navigating there.`);
                    setResultModal({
                        isOpen: true,
                        title: "Signature Already Exists",
                        content: (
                            <div>
                                <p className="mb-2">This signature was already added previously to the voucher.</p>
                                <p className="text-theme-subtle">Redirecting to the voucher details.</p>
                            </div>
                        ),
                        confirmText: "Go to Voucher",
                        voucherId: voucherId
                    });
                    return;
                }
            }

            const msg = `Failed during file processing. Error: ${errorStr}`;
            logger.error(msg);
            if (e instanceof Error && e.stack) logger.error(e.stack);
            setFeedbackMsg(`Error: ${msg}`);
        } finally {
            setIsProcessing(false);
            setShowConfirm(false);
        }
    }

    return (
        <div className="flex flex-col h-full max-w-4xl mx-auto">
            <header className="flex-shrink-0 mb-6">
                <div className="flex items-center gap-4 mb-2">
                    <button
                        onClick={onBack}
                        className="p-2.5 rounded-full bg-white border border-theme-subtle hover:bg-bg-input-readonly transition-all text-theme-light hover:text-theme-primary shadow-sm active:scale-95"
                        title="Cancel"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 19l-7-7m0 0l7-7m-7 7h18" />
                        </svg>
                    </button>
                    <h1 className="text-2xl font-bold text-theme-primary">Receive & Process</h1>
                </div>
                <p className="text-theme-light ml-14">Process transfers, signature requests, or responses you have received.</p>
            </header>

            <div className="flex-grow">
                <form onSubmit={handleProcessClick} className="space-y-6">
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
                                <p className="text-theme-light mb-4">Drag & Drop your '.transfer', '.ask', or '.sig' file here</p>
                                <p className="text-theme-light text-sm mb-4">or</p>
                                <Button type="button" onClick={handleFileSelect}>Select Bundle File</Button>
                            </div>
                        )}
                     </div>

                     {(bundlePath || droppedFileContent) && (
                         <Button size="lg" type="submit" className="w-full">
                             Process File
                         </Button>
                     )}

                </form>
            </div>

            <ConfirmationModal
                isOpen={showConfirm}
                title={`Process ${fileType === 'transfer' ? 'Transfer Bundle' : fileType === 'ask' ? 'Signature Request' : 'Signature Response'}`}
                description={
                    <p>
                        Do you want to process the file <strong>{bundleName}</strong>?<br/>
                        {fileType === 'transfer' && 'This will check for funds and add them to your wallet.'}
                        {fileType === 'ask' && 'This will open a signature request for you to review and sign.'}
                        {fileType === 'sig' && 'This will attach the signature to the corresponding voucher in your wallet.'}
                        
                        {(fileType === 'ask' || fileType === 'sig') && (
                            <div className="mt-4">
                                <label className="block text-sm font-medium text-theme-primary mb-1">
                                    Bundle Password (optional)
                                </label>
                                <input
                                    type="password"
                                    value={bundlePassword}
                                    onChange={(e) => setBundlePassword(e.target.value)}
                                    placeholder="Only if encrypted with password"
                                    className="w-full px-3 py-2 border border-theme-subtle rounded-md bg-bg-input text-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary text-sm"
                                />
                                <p className="text-[10px] text-theme-light mt-1">
                                    If the sender protected the bundle with a password, enter it here.
                                </p>
                            </div>
                        )}
                    </p>
                }
                confirmText="Yes, Process"
                onConfirm={executeReceive}
                onCancel={() => {
                    setShowConfirm(false);
                    setBundlePassword("");
                }}
                isProcessing={isProcessing}
            />

            {resultModal && (
                <ConfirmationModal
                    isOpen={resultModal.isOpen}
                    title={resultModal.title}
                    description={resultModal.content}
                    confirmText={resultModal.confirmText}
                    cancelText="Close"
                    onConfirm={() => {
                        if (resultModal.voucherId) {
                            onReceiveSuccess({
                                senderId: "",
                                transferSummary: { summableAmounts: {}, countableItems: {} },
                                involvedVouchers: [resultModal.voucherId],
                                isSignatureAttached: true,
                                voucherId: resultModal.voucherId
                            });
                        }
                        setResultModal(null);
                    }}
                    onCancel={() => {
                        setResultModal(null);
                        onBack();
                    }}
                />
            )}
        </div>
    );
}