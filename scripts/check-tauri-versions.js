#!/usr/bin/env node

/**
 * Pre-flight check to ensure Tauri Rust and NPM versions are synchronized.
 * Tauri 2 requires the Rust crate and NPM packages to be on the same major/minor version.
 */

import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

function parseVersion(versionString) {
  // Handle various version formats: "2.11.0", "~2.11.0", "^2.11.0", "=2.11.0"
  const match = versionString.match(/[\^~=]?(\d+\.\d+)/);
  if (!match) {
    throw new Error(`Invalid version format: ${versionString}`);
  }
  return match[1];
}

function getNpmTauriVersion() {
  const packageJsonPath = join(__dirname, '..', 'package.json');
  const packageJson = JSON.parse(readFileSync(packageJsonPath, 'utf-8'));
  
  const apiVersion = packageJson.dependencies['@tauri-apps/api'];
  const cliVersion = packageJson.devDependencies['@tauri-apps/cli'];
  
  if (!apiVersion || !cliVersion) {
    throw new Error('Missing @tauri-apps/api or @tauri-apps/cli in package.json');
  }
  
  return {
    api: parseVersion(apiVersion),
    cli: parseVersion(cliVersion)
  };
}

function getRustTauriVersion() {
  const cargoTomlPath = join(__dirname, '..', 'src-tauri', 'Cargo.toml');
  const cargoToml = readFileSync(cargoTomlPath, 'utf-8');
  
  // Parse tauri version from Cargo.toml
  // Look for: tauri = { version = "~2.11", ... } or tauri = "~2.11"
  const match = cargoToml.match(/tauri\s*=\s*(?:\{[^}]*version\s*=\s*["']([^"']+)["'][^}]*\}|["']([^"']+)["'])/);
  
  if (!match) {
    throw new Error('Could not find tauri version in Cargo.toml');
  }
  
  return parseVersion(match[1] || match[2]);
}

function main() {
  try {
    console.log('Checking Tauri version synchronization...');
    
    const npmVersions = getNpmTauriVersion();
    const rustVersion = getRustTauriVersion();
    
    console.log(`  NPM @tauri-apps/api: ${npmVersions.api}`);
    console.log(`  NPM @tauri-apps/cli: ${npmVersions.cli}`);
    console.log(`  Rust tauri crate: ${rustVersion}`);
    
    if (npmVersions.api !== npmVersions.cli) {
      console.error('\n❌ Error: NPM @tauri-apps/api and @tauri-apps/cli versions do not match.');
      console.error(`   API: ${npmVersions.api}, CLI: ${npmVersions.cli}`);
      process.exit(1);
    }
    
    if (npmVersions.api !== rustVersion) {
      console.error('\n❌ Error: NPM and Rust Tauri versions do not match.');
      console.error(`   NPM: ${npmVersions.api}, Rust: ${rustVersion}`);
      console.error('\n   Tauri 2 requires the Rust crate and NPM packages to be on the same major/minor version.');
      console.error('   Update package.json and src-tauri/Cargo.toml to use matching versions.');
      process.exit(1);
    }
    
    console.log('\n✅ Tauri versions are synchronized.');
    process.exit(0);
  } catch (error) {
    console.error(`\n❌ Error checking versions: ${error.message}`);
    process.exit(1);
  }
}

main();
