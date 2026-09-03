const os = require('os');
const path = require('path');
const fs = require('fs');
const { spawnSync } = require('child_process');

function getBinaryName(tool) {
  return os.platform() === 'win32' ? `${tool}.exe` : tool;
}

function resolveBinary(tool) {
  const exeName = getBinaryName(tool);

  // 1. Check local bin directory inside package if bundled
  const localBin = path.join(__dirname, '..', 'bin', exeName);
  if (fs.existsSync(localBin)) {
    return localBin;
  }

  // 2. Check user's local .datara installation directory
  const homeDir = os.homedir();
  const dataraHome = path.join(homeDir, '.datara', 'bin', exeName);
  if (fs.existsSync(dataraHome)) {
    return dataraHome;
  }

  // 3. Fallback: try looking in system PATH directly
  return exeName;
}

function runBinary(tool, args) {
  const bin = resolveBinary(tool);
  const result = spawnSync(bin, args, { stdio: 'inherit', shell: false });
  if (result.error) {
    if (result.error.code === 'ENOENT') {
      console.error(`\x1b[31m[Datara NPM] Binary '${tool}' not found.\x1b[0m`);
      console.error(`Please install the Datara toolchain:`);
      console.error(`  - Windows:     iwr https://raw.githubusercontent.com/waters1ze/datara/main/install.ps1 -useb | iex`);
      console.error(`  - macOS/Linux: curl -fsSL https://raw.githubusercontent.com/waters1ze/datara/main/install.sh | bash`);
      process.exit(1);
    }
    console.error(result.error);
    process.exit(1);
  }
  process.exit(result.status ?? 0);
}

module.exports = { runBinary, resolveBinary };
