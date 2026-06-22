import { describe, it, expect } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Helper to recursively get files
function getFiles(dir: string): string[] {
  const subdirs = fs.readdirSync(dir);
  const files = subdirs.map((subdir) => {
    const res = path.resolve(dir, subdir);
    return fs.statSync(res).isDirectory() ? getFiles(res) : res;
  });
  return files.flat();
}

describe('Tauri Command Integrity', () => {
  it('should ensure all frontend invoke calls match registered Tauri commands', () => {
    // 1. Read registered commands from src-tauri/src/lib.rs
    const libRsPath = path.resolve(__dirname, '../../../src-tauri/src/lib.rs');
    const libRsContent = fs.readFileSync(libRsPath, 'utf-8');

    // Find the tauri::generate_handler![...] block
    const handlerMatch = libRsContent.match(/generate_handler!\[([\s\S]*?)\]/);
    if (!handlerMatch) {
      throw new Error('Could not find generate_handler! block in lib.rs');
    }

    const handlerBlock = handlerMatch[1];
    
    // Clean up comments and extract command names
    const cleanBlock = handlerBlock
      .replace(/\/\/.*$/gm, '') // Remove line comments
      .replace(/\/\*[\s\S]*?\*\//g, ''); // Remove block comments

    const registeredCommands = new Set(
      cleanBlock
        .split(',')
        .map((cmd) => cmd.trim())
        .filter((cmd) => cmd.length > 0)
    );

    // 2. Scan frontend ts/tsx files for invoke calls
    const srcDir = path.resolve(__dirname, '../..');
    const allFiles = getFiles(srcDir);
    const tsFiles = allFiles.filter(
      (file) =>
        (file.endsWith('.ts') || file.endsWith('.tsx')) &&
        !file.includes('/__tests__/') &&
        !file.endsWith('.test.ts') &&
        !file.endsWith('.test.tsx') &&
        !file.includes('/test/')
    );

    const invokedCommands = new Map<string, string[]>(); // commandName -> files where it is used

    for (const file of tsFiles) {
      const content = fs.readFileSync(file, 'utf-8');
      
      const lines = content.split('\n');
      lines.forEach((line, index) => {
        // Strip line comments from the line before matching
        const cleanLine = line.split('//')[0];
        let match;
        // Recreate regex for each line since we want to find all matches in that line
        const localRegex = /invoke\s*(?:<[^>]+>)?\s*\(\s*["']([^"']+)["']/g;
        while ((match = localRegex.exec(cleanLine)) !== null) {
          const cmd = match[1];
          if (!invokedCommands.has(cmd)) {
            invokedCommands.set(cmd, []);
          }
          invokedCommands.get(cmd)!.push(`${path.relative(srcDir, file)}:L${index + 1}`);
        }
      });
    }

    // 3. Verify
    const missingCommands: string[] = [];
    for (const [cmd, locations] of invokedCommands.entries()) {
      if (!registeredCommands.has(cmd)) {
        missingCommands.push(`Command "${cmd}" is invoked but not registered in lib.rs. Locations:\n${locations.map(loc => `  - ${loc}`).join('\n')}`);
      }
    }

    if (missingCommands.length > 0) {
      console.error('Missing Tauri command registrations:', missingCommands);
    }
    expect(missingCommands).toEqual([]);
  });

  it('should ensure all frontend invoke payload keys match camelCase representations of backend Rust parameters', () => {
    // 1. Scan Rust files in src-tauri/src/commands/ for #[tauri::command]
    const commandsDir = path.resolve(__dirname, '../../../src-tauri/src/commands');
    const rustFiles = fs.readdirSync(commandsDir).filter((f) => f.endsWith('.rs'));

    interface BackendParam {
      name: string;
      isOptional: boolean;
    }

    const backendCommands = new Map<string, BackendParam[]>();

    for (const file of rustFiles) {
      const content = fs.readFileSync(path.join(commandsDir, file), 'utf-8');

      // Split content by #[tauri::command]
      const parts = content.split('#[tauri::command]');
      for (let i = 1; i < parts.length; i++) {
        const sigPart = parts[i].trim();
        const fnMatch = sigPart.match(/^(?:pub\s+)?(?:async\s+)?fn\s+([a-zA-Z0-9_]+)\s*\(([\s\S]*?)\)/);
        if (fnMatch) {
          const cmdName = fnMatch[1];
          const argsStr = fnMatch[2];

          // Parse arguments
          const params: BackendParam[] = [];

          // Split arguments by commas outside of <> brackets
          const args: string[] = [];
          let currentArg = '';
          let depth = 0;
          for (let charIndex = 0; charIndex < argsStr.length; charIndex++) {
            const char = argsStr[charIndex];
            if (char === '<' || char === '(' || char === '[') {
              depth++;
            } else if (char === '>' || char === ')' || char === ']') {
              depth--;
            }

            if (char === ',' && depth === 0) {
              args.push(currentArg.trim());
              currentArg = '';
            } else {
              currentArg += char;
            }
          }
          if (currentArg.trim()) {
            args.push(currentArg.trim());
          }

          for (const arg of args) {
            const cleanArg = arg
              .replace(/\/\*[\s\S]*?\*\//g, '')
              .replace(/\/\/.*$/gm, '')
              .trim();
            if (!cleanArg) continue;

            const colonIndex = cleanArg.indexOf(':');
            if (colonIndex === -1) continue;

            const paramName = cleanArg.substring(0, colonIndex).trim();
            const typeStr = cleanArg.substring(colonIndex + 1).trim();

            // Ignore Tauri-injected parameters
            if (
              typeStr.includes('State<') ||
              typeStr.includes("State<'") ||
              typeStr.includes('AppHandle') ||
              typeStr.includes('Window') ||
              typeStr.includes('WebviewWindow')
            ) {
              continue;
            }

            const isOptional = typeStr.startsWith('Option<') || typeStr.startsWith('Option <');
            const camelName = paramName.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());

            params.push({ name: camelName, isOptional });
          }

          backendCommands.set(cmdName, params);
        }
      }
    }

    // 2. Scan frontend TS/TSX files for invoke calls and extract their payloads
    const srcDir = path.resolve(__dirname, '../..');
    const allFiles = getFiles(srcDir);
    const tsFiles = allFiles.filter(
      (file) =>
        (file.endsWith('.ts') || file.endsWith('.tsx')) &&
        !file.includes('/__tests__/') &&
        !file.endsWith('.test.ts') &&
        !file.endsWith('.test.tsx') &&
        !file.includes('/test/')
    );

    interface Invocation {
      keys: string[];
      location: string;
    }
    const invokedCommandsWithKeys = new Map<string, Invocation[]>();

    for (const file of tsFiles) {
      const content = fs.readFileSync(file, 'utf-8');

      // Strip comments
      const cleanContent = content
        .replace(/\/\*[\s\S]*?\*\//g, '')
        .replace(/\/\/.*$/gm, '');

      let index = 0;
      while ((index = cleanContent.indexOf('invoke', index)) !== -1) {
        let pos = index + 6;
        while (pos < cleanContent.length && /\s/.test(cleanContent[pos])) {
          pos++;
        }
        if (cleanContent[pos] === '<') {
          let depth = 0;
          while (pos < cleanContent.length) {
            if (cleanContent[pos] === '<') depth++;
            else if (cleanContent[pos] === '>') {
              depth--;
              if (depth === 0) {
                pos++;
                break;
              }
            }
            pos++;
          }
        }
        while (pos < cleanContent.length && /\s/.test(cleanContent[pos])) {
          pos++;
        }
        if (cleanContent[pos] !== '(') {
          index += 6;
          continue;
        }
        pos++;

        let argsStr = '';
        let depth = 1;
        let inString = false;
        let stringChar = '';
        while (pos < cleanContent.length && depth > 0) {
          const char = cleanContent[pos];
          if (!inString && (char === '"' || char === "'" || char === '`')) {
            inString = true;
            stringChar = char;
          } else if (inString && char === stringChar && cleanContent[pos - 1] !== '\\') {
            inString = false;
          }

          if (!inString) {
            if (char === '(') depth++;
            else if (char === ')') {
              depth--;
              if (depth === 0) {
                break;
              }
            }
          }
          argsStr += char;
          pos++;
        }

        let firstCommaIndex = -1;
        let braceDepth = 0;
        let parenDepth = 0;
        let bracketDepth = 0;
        inString = false;
        stringChar = '';
        for (let i = 0; i < argsStr.length; i++) {
          const char = argsStr[i];
          if (!inString && (char === '"' || char === "'" || char === '`')) {
            inString = true;
            stringChar = char;
          } else if (inString && char === stringChar && argsStr[i - 1] !== '\\') {
            inString = false;
          }

          if (!inString) {
            if (char === '{') braceDepth++;
            else if (char === '}') braceDepth--;
            else if (char === '(') parenDepth++;
            else if (char === ')') parenDepth--;
            else if (char === '[') bracketDepth++;
            else if (char === ']') bracketDepth--;
            else if (char === ',' && braceDepth === 0 && parenDepth === 0 && bracketDepth === 0) {
              firstCommaIndex = i;
              break;
            }
          }
        }

        let commandName = '';
        let payloadStr = '';
        if (firstCommaIndex === -1) {
          commandName = argsStr.trim();
        } else {
          commandName = argsStr.substring(0, firstCommaIndex).trim();
          payloadStr = argsStr.substring(firstCommaIndex + 1).trim();
        }

        const cmdNameMatch = commandName.match(/^["']([^"']+)["']$/);
        if (cmdNameMatch) {
          const cmdName = cmdNameMatch[1];
          const keys: string[] = [];

          if (payloadStr) {
            const trimmedPayload = payloadStr.trim();
            if (trimmedPayload.startsWith('{') && trimmedPayload.endsWith('}')) {
              const objContent = trimmedPayload.substring(1, trimmedPayload.length - 1).trim();

              const parts: string[] = [];
              let currentPart = '';
              let objBraceDepth = 0;
              let objParenDepth = 0;
              let objBracketDepth = 0;
              inString = false;
              stringChar = '';

              for (let i = 0; i < objContent.length; i++) {
                const char = objContent[i];
                if (!inString && (char === '"' || char === "'" || char === '`')) {
                  inString = true;
                  stringChar = char;
                } else if (inString && char === stringChar && objContent[i - 1] !== '\\') {
                  inString = false;
                }

                if (!inString) {
                  if (char === '{') objBraceDepth++;
                  else if (char === '}') objBraceDepth--;
                  else if (char === '(') objParenDepth++;
                  else if (char === ')') objParenDepth--;
                  else if (char === '[') objBracketDepth++;
                  else if (char === ']') objBracketDepth--;

                  if (char === ',' && objBraceDepth === 0 && objParenDepth === 0 && objBracketDepth === 0) {
                    parts.push(currentPart.trim());
                    currentPart = '';
                    continue;
                  }
                }
                currentPart += char;
              }
              if (currentPart.trim()) {
                parts.push(currentPart.trim());
              }

              for (const part of parts) {
                let firstColonIndex = -1;
                let partBraceDepth = 0;
                let partParenDepth = 0;
                let partBracketDepth = 0;
                inString = false;
                stringChar = '';
                for (let i = 0; i < part.length; i++) {
                  const char = part[i];
                  if (!inString && (char === '"' || char === "'" || char === '`')) {
                    inString = true;
                    stringChar = char;
                  } else if (inString && char === stringChar && part[i - 1] !== '\\') {
                    inString = false;
                  }

                  if (!inString) {
                    if (char === '{') partBraceDepth++;
                    else if (char === '}') partBraceDepth--;
                    else if (char === '(') partParenDepth++;
                    else if (char === ')') partParenDepth--;
                    else if (char === '[') partBracketDepth++;
                    else if (char === ']') partBracketDepth--;
                    else if (char === ':' && partBraceDepth === 0 && partParenDepth === 0 && partBracketDepth === 0) {
                      firstColonIndex = i;
                      break;
                    }
                  }
                }

                let rawKey = '';
                if (firstColonIndex === -1) {
                  rawKey = part.trim();
                } else {
                  rawKey = part.substring(0, firstColonIndex).trim();
                }

                const keyCleanMatch = rawKey.match(/^["']?([a-zA-Z0-9_$]+)["']?$/);
                if (keyCleanMatch) {
                  keys.push(keyCleanMatch[1]);
                }
              }
            }
          }

          const beforeMatch = content.substring(0, index);
          const lineNum = beforeMatch.split('\n').length;
          const relativeFile = path.relative(srcDir, file);

          if (!invokedCommandsWithKeys.has(cmdName)) {
            invokedCommandsWithKeys.set(cmdName, []);
          }
          invokedCommandsWithKeys.get(cmdName)!.push({ keys, location: `${relativeFile}:L${lineNum}` });
        }

        index += 6;
      }
    }

    // 3. Compare invoked frontend payloads against backend command parameters
    const mismatches: string[] = [];

    for (const [cmdName, invocations] of invokedCommandsWithKeys.entries()) {
      const backendParams = backendCommands.get(cmdName);
      if (!backendParams) {
        // Skip check if the command is not found in backend (another test checks for unregistered commands)
        continue;
      }

      const backendParamNames = backendParams.map((p) => p.name);
      const requiredBackendParams = backendParams.filter((p) => !p.isOptional).map((p) => p.name);

      for (const invocation of invocations) {
        // Find if there are any extra keys in frontend that are not in backend
        const extraKeys = invocation.keys.filter((key) => !backendParamNames.includes(key));
        if (extraKeys.length > 0) {
          mismatches.push(
            `Command "${cmdName}" at ${invocation.location} sent extra parameters: [${extraKeys.join(
              ', '
            )}]. Expected backend parameters: [${backendParamNames.join(', ')}].`
          );
        }

        // Find if there are any missing required parameters in frontend
        const missingKeys = requiredBackendParams.filter((param) => !invocation.keys.includes(param));
        if (missingKeys.length > 0) {
          mismatches.push(
            `Command "${cmdName}" at ${invocation.location} is missing required parameters: [${missingKeys.join(
              ', '
            )}]. Provided parameters: [${invocation.keys.join(', ')}].`
          );
        }
      }
    }

    if (mismatches.length > 0) {
      console.error('Tauri parameter casing or mismatch errors found:\n', mismatches.join('\n'));
    }
    expect(mismatches).toEqual([]);
  });

  it('should ensure no Tauri command accepts raw core models as parameters', () => {
    const commandsDir = path.resolve(__dirname, '../../../src-tauri/src/commands');
    const rustFiles = fs.readdirSync(commandsDir).filter((f) => f.endsWith('.rs'));

    const blacklistedTypes = [
      'Voucher',
      'VoucherStandard',
      'PublicProfile',
      'ValueDefinition',
      'Collateral',
      'Transaction',
      'TrapData',
      'VoucherSignature',
      'Address',
    ];

    const violations: string[] = [];

    for (const file of rustFiles) {
      const content = fs.readFileSync(path.join(commandsDir, file), 'utf-8');

      const parts = content.split('#[tauri::command]');
      for (let i = 1; i < parts.length; i++) {
        const sigPart = parts[i].trim();
        const fnMatch = sigPart.match(/^(?:pub\s+)?(?:async\s+)?fn\s+([a-zA-Z0-9_]+)\s*\(([\s\S]*?)\)/);
        if (fnMatch) {
          const cmdName = fnMatch[1];
          const argsStr = fnMatch[2];

          const args: string[] = [];
          let currentArg = '';
          let depth = 0;
          for (let charIndex = 0; charIndex < argsStr.length; charIndex++) {
            const char = argsStr[charIndex];
            if (char === '<' || char === '(' || char === '[') {
              depth++;
            } else if (char === '>' || char === ')' || char === ']') {
              depth--;
            }

            if (char === ',' && depth === 0) {
              args.push(currentArg.trim());
              currentArg = '';
            } else {
              currentArg += char;
            }
          }
          if (currentArg.trim()) {
            args.push(currentArg.trim());
          }

          for (const arg of args) {
            const cleanArg = arg
              .replace(/\/\*[\s\S]*?\*\//g, '')
              .replace(/\/\/.*$/gm, '')
              .trim();
            if (!cleanArg) continue;

            const colonIndex = cleanArg.indexOf(':');
            if (colonIndex === -1) continue;

            const paramName = cleanArg.substring(0, colonIndex).trim();
            const typeStr = cleanArg.substring(colonIndex + 1).trim();

            for (const blacklisted of blacklistedTypes) {
              const typeRegex = new RegExp(`(?<!Frontend)\\b${blacklisted}\\b`);
              if (typeRegex.test(typeStr)) {
                violations.push(
                  `Command "${cmdName}" in ${file} accepts raw core model type "${typeStr}" for parameter "${paramName}". Please use a Frontend DTO (e.g. Frontend${blacklisted}) instead.`
                );
              }
            }
          }
        }
      }
    }

    if (violations.length > 0) {
      console.error('Tauri command parameters accepting raw core models:\n', violations.join('\n'));
    }
    expect(violations).toEqual([]);
  });
});

