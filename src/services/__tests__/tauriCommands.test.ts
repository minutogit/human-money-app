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
});
