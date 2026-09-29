#!/usr/bin/env node
'use strict';

// Runs the ironwork build that matches this machine; the package carries one per target.
const { spawnSync } = require('node:child_process');
const { existsSync } = require('node:fs');
const { join } = require('node:path');

const TARGETS = {
  'darwin-arm64': 'aarch64-apple-darwin',
  'darwin-x64': 'x86_64-apple-darwin',
  'linux-arm64': 'aarch64-unknown-linux-musl',
  'linux-x64': 'x86_64-unknown-linux-musl',
  'win32-x64': 'x86_64-pc-windows-msvc',
};

const platform = `${process.platform}-${process.arch}`;
const target = TARGETS[platform];
const exe = target && join(__dirname, '..', 'dist', target, process.platform === 'win32' ? 'ironwork.exe' : 'ironwork');
if (!exe || !existsSync(exe)) {
  console.error(`ironwork: this package has no build for ${platform}. Install it with \`cargo install ironwork\` instead.`);
  process.exit(1);
}

const run = spawnSync(exe, process.argv.slice(2), { stdio: 'inherit' });
if (run.error) {
  console.error(`ironwork: ${run.error.message}`);
  process.exit(1);
}
if (run.signal) process.kill(process.pid, run.signal);
process.exit(run.status);
