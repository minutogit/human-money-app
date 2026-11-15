#!/usr/bin/env node
// scripts/ui-tour.mjs
// Automated and Interactive UI Tour & Audit System for HuMoCo Wallet.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn, execSync } from 'node:child_process';
import { TOUR_STEPS, getTauriMockInitScript } from './tour-catalog.mjs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const APP_DIR = path.resolve(__dirname, '..');
const TEST_RESULTS_DIR = path.resolve(APP_DIR, 'test-results', 'tour');

const args = process.argv.slice(2);
const isAuto = args.includes('--auto');
const isHeadless = isAuto || args.includes('--headless') || (!process.env.DISPLAY && !process.env.WAYLAND_DISPLAY);
const modeFlag = args.find(a => a.startsWith('--mode='));
const modeFlagIdx = args.indexOf('--mode');
const rawModeArg = modeFlag
    ? modeFlag.split('=')[1]
    : (modeFlagIdx >= 0 && args[modeFlagIdx + 1] ? args[modeFlagIdx + 1] : null);
const VALID_MODES = ['simple', 'expert', 'both'];
const requestedMode = VALID_MODES.includes(rawModeArg) ? rawModeArg : (isAuto ? 'both' : 'simple');
const auditModes = requestedMode === 'both' ? ['simple', 'expert'] : [requestedMode];
const initialHudMode = requestedMode === 'expert' ? 'expert' : 'simple';
const targetStepArg = args.find((a, idx) => !a.startsWith('-') && !(modeFlagIdx >= 0 && idx === modeFlagIdx + 1));

function findBrowser() {
    const candidates = [
        process.env.CHROME_BIN,
        process.env.CHROMIUM_BIN,
        '/usr/bin/chromium',
        '/usr/bin/google-chrome',
        '/usr/bin/chromium-browser',
        '/usr/bin/brave-browser',
        '/usr/bin/brave'
    ].filter(Boolean);

    for (const p of candidates) {
        if (fs.existsSync(p)) return p;
    }

    try {
        const out = execSync('which chromium || which google-chrome || which chromium-browser', { encoding: 'utf8' }).trim();
        if (out && fs.existsSync(out)) return out;
    } catch {
        // Ignored
    }
    return null;
}

async function ensureViteServer(url = 'http://localhost:1420') {
    try {
        const res = await fetch(url);
        if (res.ok || res.status === 200 || res.status === 404) {
            console.log(`[Tour] Vite server already running at ${url}`);
            return null;
        }
    } catch {
        // Not running
    }

    console.log(`[Tour] Starting Vite development server on port 1420...`);
    const viteProcess = spawn('npx', ['vite', '--port', '1420'], {
        cwd: APP_DIR,
        stdio: 'pipe',
        env: { ...process.env, BROWSER: 'none' }
    });

    const start = Date.now();
    while (Date.now() - start < 20000) {
        try {
            const res = await fetch(url);
            if (res.ok || res.status === 200) {
                console.log(`[Tour] Vite server ready at ${url}`);
                return viteProcess;
            }
        } catch {
            // Wait and retry
        }
        await new Promise(r => setTimeout(r, 400));
    }
    throw new Error(`Timeout waiting for Vite dev server at ${url}`);
}

class CdpSession {
    constructor(wsUrl) {
        this.ws = new WebSocket(wsUrl);
        this.id = 1;
        this.pending = new Map();
        this.eventListeners = new Map();
    }

    async connect() {
        return new Promise((resolve, reject) => {
            this.ws.onopen = resolve;
            this.ws.onerror = reject;
            this.ws.onmessage = (event) => {
                try {
                    const msg = JSON.parse(event.data);
                    if (msg.id && this.pending.has(msg.id)) {
                        const { resolve, reject } = this.pending.get(msg.id);
                        this.pending.delete(msg.id);
                        if (msg.error) {
                            reject(new Error(msg.error.message || JSON.stringify(msg.error)));
                        } else {
                            resolve(msg.result);
                        }
                    } else if (msg.method) {
                        const listeners = this.eventListeners.get(msg.method) || [];
                        for (const fn of listeners) {
                            try { fn(msg.params); } catch (e) { console.error(e); }
                        }
                    }
                } catch (e) {
                    console.error('[CDP Parse Error]', e);
                }
            };
        });
    }

    async send(method, params = {}) {
        const id = this.id++;
        return new Promise((resolve, reject) => {
            this.pending.set(id, { resolve, reject });
            this.ws.send(JSON.stringify({ id, method, params }));
        });
    }

    on(event, handler) {
        if (!this.eventListeners.has(event)) {
            this.eventListeners.set(event, []);
        }
        this.eventListeners.get(event).push(handler);
    }

    async evaluate(expression) {
        const res = await this.send('Runtime.evaluate', {
            expression,
            returnByValue: true,
            awaitPromise: true
        });
        return res?.result?.value;
    }

    close() {
        try { this.ws.close(); } catch { /* Ignore */ }
    }
}

function getHudScript(initialStepId = '1.1', initialUiMode = 'simple') {
    const stepsJson = JSON.stringify(TOUR_STEPS);
    return `
(function() {
    if (window.__HUMOCO_HUD_INITIALIZED__) return;
    window.__HUMOCO_HUD_INITIALIZED__ = true;

    const STEPS = ${stepsJson};
    let currentIndex = STEPS.findIndex(s => s.id === "${initialStepId}");
    if (currentIndex < 0) currentIndex = 0;

    let isMobile = false;
    let isDark = false;
    let uiMode = "${initialUiMode}";
    try {
        isDark = document.documentElement?.classList?.contains('dark') ?? false;
    } catch (e) {
        // Safe fallback when documentElement is not yet available
    }
    let scenarios = ['normal', 'empty', 'error'];
    let currentScenarioIdx = 0;

    function applyStep(index) {
        if (index < 0 || index >= STEPS.length) return;
        currentIndex = index;
        const step = STEPS[currentIndex];
        const effectiveUiMode = step.uiMode || uiMode;
        try { window.location.hash = '#' + step.id; } catch (e) {}

        if (window.__HUMOCO_TOUR__) {
            window.__HUMOCO_TOUR__.setState({
                appState: step.appState,
                isAuthenticated: step.auth,
                isSessionActive: step.auth,
                isForkLocked: false,
                isRecoveryRequired: false,
                ...(step.sessionOverrides || {}),
                settingsTab: step.settingsTab,
                uiMode: effectiveUiMode
            });
        }

        if (step.postAction) {
            try {
                if (typeof step.postAction === 'function') {
                    step.postAction();
                } else if (typeof step.postAction === 'string') {
                    eval(step.postAction);
                }
            } catch (err) {
                console.warn('[Tour postAction error]', err);
            }
        }

        renderHud();
        try {
            window.dispatchEvent(new CustomEvent('humoco:tour:step_changed', { detail: { step, index } }));
        } catch (e) {}
    }

    // Register window.__HUMOCO_TOUR_HUD__ BEFORE any DOM access so it is always defined on window
    window.__HUMOCO_TOUR_HUD__ = {
        goToStep: (idx) => applyStep(idx),
        goToStepId: (id) => {
            const idx = STEPS.findIndex(s => s.id === id);
            if (idx >= 0) applyStep(idx);
        },
        next: () => {
            if (currentIndex < STEPS.length - 1) applyStep(currentIndex + 1);
        },
        prev: () => {
            if (currentIndex > 0) applyStep(currentIndex - 1);
        },
        getCurrentStep: () => ({ index: currentIndex, step: STEPS[currentIndex] }),
        toggleDark: () => {
            isDark = !isDark;
            try {
                if (document.documentElement) {
                    document.documentElement.classList.toggle('dark', isDark);
                }
            } catch (e) {}
            renderHud();
        },
        toggleMobile: () => {
            isMobile = !isMobile;
            try {
                if (document.body) {
                    if (isMobile) {
                        document.body.style.maxWidth = '390px';
                        document.body.style.margin = '0 auto';
                        document.body.style.boxShadow = '0 0 40px rgba(0,0,0,0.5)';
                    } else {
                        document.body.style.maxWidth = '';
                        document.body.style.margin = '';
                        document.body.style.boxShadow = '';
                    }
                }
            } catch (e) {}
            renderHud();
        },
        toggleUiMode: (forcedMode) => {
            uiMode = (forcedMode === 'simple' || forcedMode === 'expert')
                ? forcedMode
                : (uiMode === 'expert' ? 'simple' : 'expert');
            if (window.__HUMOCO_TOUR__) {
                window.__HUMOCO_TOUR__.setUiMode(uiMode);
            }
            renderHud();
        },
        getUiMode: () => uiMode,
        cycleScenario: () => {
            currentScenarioIdx = (currentScenarioIdx + 1) % scenarios.length;
            const nextSc = scenarios[currentScenarioIdx];
            if (window.__HUMOCO_SET_SCENARIO__) {
                window.__HUMOCO_SET_SCENARIO__(nextSc);
            }
            renderHud();
        },
        captureNote: () => {
            const cur = STEPS[currentIndex];
            const note = prompt("Notiz / Feedback für Step [" + cur.id + "] " + cur.title + ":");
            if (note) {
                console.info("[TOUR NOTE] [" + cur.id + "] " + note);
                alert("Notiz erfasst für Step " + cur.id + ":\\n" + note);
            }
        }
    };

    function renderHud() {
        if (!document.body) return;
        let container = document.getElementById('__humoco_tour_hud__');
        if (!container) {
            container = document.createElement('div');
            container.id = '__humoco_tour_hud__';
            container.style.cssText = [
                'position: fixed',
                'top: 14px',
                'right: 14px',
                'z-index: 9999999',
                'display: flex',
                'align-items: center',
                'gap: 8px',
                'background: rgba(15, 23, 42, 0.94)',
                'backdrop-filter: blur(10px)',
                '-webkit-backdrop-filter: blur(10px)',
                'color: #f8fafc',
                'border: 1px solid rgba(255, 255, 255, 0.2)',
                'border-radius: 9999px',
                'padding: 6px 14px',
                'font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
                'font-size: 13px',
                'font-weight: 500',
                'box-shadow: 0 10px 30px -5px rgba(0,0,0,0.5)',
                'user-select: none',
                'transition: all 0.2s ease'
            ].join(';');
            document.body.appendChild(container);
        }

        const cur = STEPS[currentIndex] || STEPS[0];
        const stepLabel = "[" + cur.id + "] " + cur.title + " (" + (currentIndex + 1) + "/" + STEPS.length + ")";
        const scenarioLabel = scenarios[currentScenarioIdx];
        const effectiveUiMode = cur.uiMode || uiMode;

        container.innerHTML = \`
            <button id="__tour_prev__" title="Zurück (Alt+Links)" style="background: rgba(255,255,255,0.12); border:none; color:white; border-radius:50%; width:24px; height:24px; cursor:pointer; display:flex; align-items:center; justify-content:center; font-size:11px;">◀</button>
            <span id="__tour_label__" title="\${cur.description}" style="cursor:default; max-width:260px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; padding: 0 4px; font-weight:700; font-size:12px; color:#38bdf8;">\${stepLabel}</span>
            <button id="__tour_next__" title="Weiter (Alt+Rechts)" style="background: rgba(255,255,255,0.12); border:none; color:white; border-radius:50%; width:24px; height:24px; cursor:pointer; display:flex; align-items:center; justify-content:center; font-size:11px;">▶</button>
            <div style="width:1px; height:18px; background:rgba(255,255,255,0.2); margin:0 2px;"></div>
            <button id="__tour_uimode__" title="UI-Modus wechseln (Taste e)" style="background:rgba(255,255,255,0.12); border:none; color:#e2e8f0; border-radius:6px; font-size:11px; padding:3px 7px; cursor:pointer;">\${effectiveUiMode === 'expert' ? '🔧 Erweitert' : '✨ Einfach'}</button>
            <button id="__tour_dark__" title="Dark / Light Toggle" style="background:none; border:none; color:white; cursor:pointer; font-size:14px; padding:2px;">\${isDark ? '☀️' : '🌙'}</button>
            <button id="__tour_mobile__" title="Mobile / Desktop Viewport" style="background:none; border:none; color:white; cursor:pointer; font-size:14px; padding:2px;">\${isMobile ? '🖥️' : '📱'}</button>
            <button id="__tour_scenario__" title="Daten-Szenario wechseln" style="background:rgba(255,255,255,0.12); border:none; color:#e2e8f0; border-radius:6px; font-size:11px; padding:3px 7px; cursor:pointer;">📦 \${scenarioLabel}</button>
            <button id="__tour_note__" title="Notiz / Bug erfassen" style="background:none; border:none; color:white; cursor:pointer; font-size:14px; padding:2px;">📝</button>
        \`;

        const prevBtn = document.getElementById('__tour_prev__');
        if (prevBtn) prevBtn.onclick = () => window.__HUMOCO_TOUR_HUD__.prev();
        const nextBtn = document.getElementById('__tour_next__');
        if (nextBtn) nextBtn.onclick = () => window.__HUMOCO_TOUR_HUD__.next();
        const uiModeBtn = document.getElementById('__tour_uimode__');
        if (uiModeBtn) uiModeBtn.onclick = () => window.__HUMOCO_TOUR_HUD__.toggleUiMode();
        const darkBtn = document.getElementById('__tour_dark__');
        if (darkBtn) darkBtn.onclick = () => window.__HUMOCO_TOUR_HUD__.toggleDark();
        const mobBtn = document.getElementById('__tour_mobile__');
        if (mobBtn) mobBtn.onclick = () => window.__HUMOCO_TOUR_HUD__.toggleMobile();
        const scBtn = document.getElementById('__tour_scenario__');
        if (scBtn) scBtn.onclick = () => window.__HUMOCO_TOUR_HUD__.cycleScenario();
        const noteBtn = document.getElementById('__tour_note__');
        if (noteBtn) noteBtn.onclick = () => window.__HUMOCO_TOUR_HUD__.captureNote();
    }

    window.addEventListener('keydown', (e) => {
        const isInput = ['INPUT', 'TEXTAREA', 'SELECT'].includes(e.target.tagName);
        if (e.altKey && e.key === 'ArrowRight') {
            e.preventDefault();
            window.__HUMOCO_TOUR_HUD__.next();
        } else if (e.altKey && e.key === 'ArrowLeft') {
            e.preventDefault();
            window.__HUMOCO_TOUR_HUD__.prev();
        } else if (!isInput && e.key === 'ArrowRight') {
            window.__HUMOCO_TOUR_HUD__.next();
        } else if (!isInput && e.key === 'ArrowLeft') {
            window.__HUMOCO_TOUR_HUD__.prev();
        } else if (!isInput && (e.key === 'e' || e.key === 'E')) {
            window.__HUMOCO_TOUR_HUD__.toggleUiMode();
        }
    });

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', () => {
            renderHud();
            setTimeout(() => applyStep(currentIndex), 400);
        });
    } else {
        renderHud();
        setTimeout(() => applyStep(currentIndex), 400);
    }
})();
`;
}

async function runTour() {
    const browserPath = findBrowser();
    if (!browserPath) {
        console.error('Error: No supported browser found (chromium, google-chrome).');
        process.exit(1);
    }

    console.log(`[Tour] Using browser binary: ${browserPath}`);
    console.log(`[Tour] Mode: ${isAuto ? 'Automated Audit (--auto)' : 'Interactive Guided Tour'}`);
    if (isHeadless && !isAuto) {
        console.log(`[Tour] Headless environment detected (no DISPLAY). Running in background.`);
        console.log(`[Tour] You can open http://localhost:1420 in any external browser to interact.`);
    }

    const viteProcess = await ensureViteServer();

    const cdpPort = 9333 + Math.floor(Math.random() * 500);
    const userDataDir = path.join('/tmp', `humoco-tour-profile-${Date.now()}`);

    const chromeArgs = [
        `--remote-debugging-port=${cdpPort}`,
        '--remote-debugging-address=127.0.0.1',
        `--user-data-dir=${userDataDir}`,
        '--no-sandbox',
        '--disable-gpu',
        '--disable-dev-shm-usage',
        '--disable-crash-reporter',
        '--disable-breakpad',
        '--no-first-run',
        '--no-default-browser-check',
        '--window-size=1280,800',
        'about:blank'
    ];

    if (isHeadless) {
        chromeArgs.push('--headless=new');
    }

    console.log(`[Tour] Launching browser on CDP port ${cdpPort}...`);
    const chromeProcess = spawn(browserPath, chromeArgs, { stdio: 'ignore' });

    const cleanup = () => {
        try { chromeProcess.kill(); } catch { /* Ignore */ }
        try { if (viteProcess) viteProcess.kill(); } catch { /* Ignore */ }
        try { fs.rmSync(userDataDir, { recursive: true, force: true }); } catch { /* Ignore */ }
    };

    process.on('SIGINT', () => { cleanup(); process.exit(0); });
    process.on('SIGTERM', () => { cleanup(); process.exit(0); });
    process.on('exit', cleanup);

    // Wait for CDP endpoint
    let cdpWsUrl = null;
    const startCdpWait = Date.now();
    while (Date.now() - startCdpWait < 10000) {
        try {
            const targetsRes = await fetch(`http://127.0.0.1:${cdpPort}/json/list`);
            const targets = await targetsRes.json();
            const pageTarget = targets.find(t => t.type === 'page');
            if (pageTarget?.webSocketDebuggerUrl) {
                cdpWsUrl = pageTarget.webSocketDebuggerUrl;
                break;
            }
        } catch {
            // Retry
        }
        await new Promise(r => setTimeout(r, 200));
    }

    if (!cdpWsUrl) {
        console.error('[Tour] Failed to connect to Chromium CDP endpoint.');
        cleanup();
        process.exit(1);
    }

    console.log(`[Tour] Connected to CDP: ${cdpWsUrl}`);
    const cdp = new CdpSession(cdpWsUrl);
    await cdp.connect();

    await cdp.send('Page.enable');
    await cdp.send('Runtime.enable');
    await cdp.send('DOM.enable');

    const initialStep = targetStepArg || '1.1';
    const initScript = getTauriMockInitScript('normal') + '\n' + getHudScript(initialStep, initialHudMode);
    await cdp.send('Page.addScriptToEvaluateOnNewDocument', { source: initScript });

    // Global collectors for current step
    let currentStepErrors = [];
    let currentStepExceptions = [];

    cdp.on('Runtime.consoleAPICalled', (params) => {
        if (params.type === 'error') {
            const text = params.args.map(a => a.value || a.description || '').join(' ');
            currentStepErrors.push(text);
        }
    });

    cdp.on('Runtime.exceptionThrown', (params) => {
        const text = params.exceptionDetails?.exception?.description || params.exceptionDetails?.text || 'Unknown exception';
        currentStepExceptions.push(text);
    });

    console.log(`[Tour] Navigating to http://localhost:1420 ...`);
    await cdp.send('Page.navigate', { url: 'http://localhost:1420' });

    console.log(`[Tour] Waiting for React application to mount...`);
    await cdp.evaluate(`new Promise(resolve => {
        const start = Date.now();
        const check = () => {
            const root = document.getElementById('root');
            if (root && root.children.length > 0 && window.__HUMOCO_TOUR__) {
                resolve(true);
            } else if (Date.now() - start > 10000) {
                resolve(false);
            } else {
                setTimeout(check, 100);
            }
        };
        check();
    })`);
    await new Promise(r => setTimeout(r, 800));

    if (isAuto) {
        console.log(`\n======================================================================`);
        console.log(`   RUNNING AUTOMATED UI TOUR & AUDIT (${TOUR_STEPS.length} STEPS x ${auditModes.join(' + ')})`);
        console.log(`======================================================================\n`);

        fs.mkdirSync(TEST_RESULTS_DIR, { recursive: true });

        const results = [];
        let passedCount = 0;
        let warningCount = 0;
        let failedCount = 0;

        for (const auditMode of auditModes) {
            console.log(`\n--- UI Mode: ${auditMode.toUpperCase()} ---`);

            for (let i = 0; i < TOUR_STEPS.length; i++) {
                const step = TOUR_STEPS[i];
                if (step.uiMode && step.uiMode !== auditMode) {
                    continue;
                }

                const effectiveUiMode = step.uiMode || auditMode;
                currentStepErrors = [];
                currentStepExceptions = [];

                process.stdout.write(`[${auditMode}] Step [${step.id}] ${step.title.padEnd(40, ' ')} ... `);

                // Navigate to step via HUD and ensure TourBridge state is applied directly
                const stepPayload = JSON.stringify({
                    appState: step.appState,
                    isAuthenticated: step.auth,
                    isSessionActive: step.auth,
                    isForkLocked: false,
                    isRecoveryRequired: false,
                    ...(step.sessionOverrides || {}),
                    settingsTab: step.settingsTab,
                    uiMode: effectiveUiMode
                });

                await cdp.evaluate(`
                    if (window.__HUMOCO_TOUR_HUD__) {
                        if (window.__HUMOCO_TOUR_HUD__.toggleUiMode) {
                            window.__HUMOCO_TOUR_HUD__.toggleUiMode(${JSON.stringify(effectiveUiMode)});
                        }
                        window.__HUMOCO_TOUR_HUD__.goToStepId("${step.id}");
                    }
                    if (window.__HUMOCO_TOUR__) {
                        window.__HUMOCO_TOUR__.setState(${stepPayload});
                    }
                `);

                // Wait for DOM to stabilize
                await new Promise(r => setTimeout(r, 650));

                // Check horizontal overflow
                const hasOverflow = await cdp.evaluate('document.documentElement.scrollWidth > window.innerWidth');

                // Capture screenshot
                const screenshotFile = effectiveUiMode === 'expert' ? `expert-${step.id}.png` : `${step.id}.png`;
                const screenshotData = await cdp.send('Page.captureScreenshot', { format: 'png' });
                const screenshotPath = path.join(TEST_RESULTS_DIR, screenshotFile);
                fs.writeFileSync(screenshotPath, Buffer.from(screenshotData.data, 'base64'));

                const stepResult = {
                    id: step.id,
                    uiMode: effectiveUiMode,
                    chapter: step.chapter,
                    title: step.title,
                    file: step.file,
                    description: step.description,
                    auth: step.auth,
                    screenshotFile,
                    screenshot: `test-results/tour/${screenshotFile}`,
                    hasOverflow: !!hasOverflow,
                    errors: [...currentStepErrors],
                    exceptions: [...currentStepExceptions],
                    status: 'PASS'
                };

                if (currentStepExceptions.length > 0 || currentStepErrors.length > 0) {
                    stepResult.status = 'FAIL';
                    failedCount++;
                    warningCount++;
                    console.log(`❌ FAIL (${currentStepErrors.length} errors, ${currentStepExceptions.length} exceptions)`);
                } else if (hasOverflow) {
                    stepResult.status = 'WARN';
                    warningCount++;
                    console.log(`⚠️ WARN (horizontal scroll overflow detected)`);
                } else {
                    passedCount++;
                    console.log(`✅ OK`);
                }

                results.push(stepResult);
            }
        }

        // Write JSON audit report
        const reportJsonPath = path.join(TEST_RESULTS_DIR, 'audit-report.json');
        fs.writeFileSync(reportJsonPath, JSON.stringify({
            timestamp: new Date().toISOString(),
            modes: auditModes,
            totalSteps: results.length,
            passed: passedCount,
            failed: failedCount,
            warningsOrFails: warningCount,
            results
        }, null, 2));

        // Write Markdown audit report
        const reportMdPath = path.join(TEST_RESULTS_DIR, 'audit-report.md');
        let md = `# HuMoCo UI Tour & Audit Report\n\n`;
        md += `**Date:** ${new Date().toLocaleString('de-DE')}  \n`;
        md += `**Modes:** ${auditModes.join(', ')}  \n`;
        md += `**Total Steps:** ${results.length} | **Passed:** ${passedCount} | **Failed:** ${failedCount} | **Warnings/Failures:** ${warningCount}\n\n`;
        md += `## Step Summary\n\n`;
        md += `| Step | Mode | Title | Status | Component | Overflow | Errors | Screenshot |\n`;
        md += `| :---: | :---: | :--- | :---: | :--- | :---: | :---: | :---: |\n`;

        for (const r of results) {
            const statusBadge = r.status === 'PASS' ? '✅ PASS' : (r.status === 'WARN' ? '⚠️ WARN' : '❌ FAIL');
            const overflowStr = r.hasOverflow ? '⚠️ Ja' : 'Nein';
            const errorCountStr = (r.errors.length + r.exceptions.length) > 0 ? `❌ ${r.errors.length + r.exceptions.length}` : '0';
            md += `| **${r.id}** | ${r.uiMode} | ${r.title} | ${statusBadge} | \`${r.file}\` | ${overflowStr} | ${errorCountStr} | [Screenshot](${r.screenshotFile}) |\n`;
        }

        md += `\n## Detailed Observations\n\n`;
        for (const r of results) {
            if (r.errors.length > 0 || r.exceptions.length > 0 || r.hasOverflow) {
                md += `### [${r.uiMode}] Step [${r.id}] ${r.title}\n`;
                md += `- **File:** \`${r.file}\`\n`;
                md += `- **Description:** ${r.description}\n`;
                if (r.hasOverflow) {
                    md += `- ⚠️ **Horizontal Overflow:** \`document.documentElement.scrollWidth > window.innerWidth\`\n`;
                }
                if (r.errors.length > 0) {
                    md += `- **Console Errors:**\n\`\`\`\n${r.errors.join('\n')}\n\`\`\`\n`;
                }
                if (r.exceptions.length > 0) {
                    md += `- **Uncaught Exceptions:**\n\`\`\`\n${r.exceptions.join('\n')}\n\`\`\`\n`;
                }
                md += `\n`;
            }
        }

        fs.writeFileSync(reportMdPath, md);

        console.log(`\n======================================================================`);
        console.log(`   AUDIT FINISHED: ${passedCount}/${results.length} PASSED (${failedCount} FAILED)`);
        console.log(`   Report JSON: ${reportJsonPath}`);
        console.log(`   Report MD:   ${reportMdPath}`);
        console.log(`   Screenshots: ${TEST_RESULTS_DIR}/`);
        console.log(`======================================================================\n`);

        cleanup();
        process.exit(failedCount > 0 ? 1 : 0);
    } else {
        // Interactive guided tour
        console.log(`\n======================================================================`);
        console.log(`   HuMoCo INTERACTIVE GUIDED TOUR STARTED`);
        console.log(`======================================================================`);
        console.log(`   - Browser HUD pill is active in the top-right corner.`);
        console.log(`   - Terminal Controls:`);
        console.log(`       [→] / [Enter] / [n] : Nächster Schritt`);
        console.log(`       [←] / [p]           : Vorheriger Schritt`);
        console.log(`       [e]                 : UI-Modus wechseln (Einfach / Erweitert)`);
        console.log(`       [d]                 : Dark / Light Mode umschalten`);
        console.log(`       [m]                 : Mobile (390px) / Desktop Toggle`);
        console.log(`       [s]                 : Daten-Szenario wechseln (normal/empty/error)`);
        console.log(`       [q]                 : Tour beenden`);
        console.log(`======================================================================\n`);

        if (process.stdin.isTTY) {
            process.stdin.setRawMode(true);
            process.stdin.resume();
            process.stdin.setEncoding('utf8');

            process.stdin.on('data', async (key) => {
                if (key === '\u0003' || key === 'q') {
                    console.log('\n[Tour] Tour beendet. Auf Wiedersehen!');
                    cleanup();
                    process.exit(0);
                } else if (key === '\u001b[C' || key === '\r' || key === 'n') {
                    await cdp.evaluate('window.__HUMOCO_TOUR_HUD__ && window.__HUMOCO_TOUR_HUD__.next()');
                    const info = await cdp.evaluate('window.__HUMOCO_TOUR_HUD__ && window.__HUMOCO_TOUR_HUD__.getCurrentStep()');
                    if (info?.step) {
                        console.log(`▶ [${info.step.id}] ${info.step.title} (${info.index + 1}/${TOUR_STEPS.length})`);
                    }
                } else if (key === '\u001b[D' || key === 'p') {
                    await cdp.evaluate('window.__HUMOCO_TOUR_HUD__ && window.__HUMOCO_TOUR_HUD__.prev()');
                    const info = await cdp.evaluate('window.__HUMOCO_TOUR_HUD__ && window.__HUMOCO_TOUR_HUD__.getCurrentStep()');
                    if (info?.step) {
                        console.log(`◀ [${info.step.id}] ${info.step.title} (${info.index + 1}/${TOUR_STEPS.length})`);
                    }
                } else if (key === 'e' || key === 'E') {
                    await cdp.evaluate('window.__HUMOCO_TOUR_HUD__ && window.__HUMOCO_TOUR_HUD__.toggleUiMode()');
                    const mode = await cdp.evaluate('window.__HUMOCO_TOUR_HUD__ && window.__HUMOCO_TOUR_HUD__.getUiMode()');
                    console.log(`🎛️ UI-Modus umgeschaltet: ${mode}`);
                } else if (key === 'd') {
                    await cdp.evaluate('window.__HUMOCO_TOUR_HUD__ && window.__HUMOCO_TOUR_HUD__.toggleDark()');
                    console.log('🌙 Dark/Light Mode umgeschaltet.');
                } else if (key === 'm') {
                    await cdp.evaluate('window.__HUMOCO_TOUR_HUD__ && window.__HUMOCO_TOUR_HUD__.toggleMobile()');
                    console.log('📱 Mobile/Desktop Viewport umgeschaltet.');
                } else if (key === 's') {
                    await cdp.evaluate('window.__HUMOCO_TOUR_HUD__ && window.__HUMOCO_TOUR_HUD__.cycleScenario()');
                    console.log('📦 Szenario gewechselt.');
                }
            });
        }
    }
}

runTour().catch((err) => {
    console.error('[Tour Error]', err);
    process.exit(1);
});
