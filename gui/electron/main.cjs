const { app, BrowserWindow, ipcMain, dialog } = require('electron');
const path = require('node:path');
const fs = require('node:fs/promises');
const { existsSync } = require('node:fs');
const { spawn } = require('node:child_process');

const root = path.resolve(__dirname, '..', '..');
const productsRoot = path.join(root, 'products');
const secretKey = /(PASSWORD|TOKEN|SECRET|KEY)/i;

function validProduct(id) {
  return /^[a-z0-9][a-z0-9_-]*$/.test(id);
}
function productDir(id) {
  if (!validProduct(id)) throw new Error('Produto inválido.');
  return path.join(productsRoot, id);
}
function parseEnv(text) {
  return text.split(/\r?\n/).map((line, index) => {
    const match = line.match(/^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)\s*$/);
    if (!match) return { line, index, key: null };
    return { line, index, key: match[1], value: match[2] };
  });
}
async function readProducts() {
  await fs.mkdir(productsRoot, { recursive: true });
  const entries = await fs.readdir(productsRoot, { withFileTypes: true });
  const products = [];
  for (const entry of entries.filter(e => e.isDirectory()).sort((a, b) => a.name.localeCompare(b.name))) {
    if (!validProduct(entry.name)) continue;
    const dir = productDir(entry.name);
    const files = await fs.readdir(dir);
    if (!files.some(name => ['.env', '.env.example', 'apps.json', 'build.env'].includes(name))) continue;
    let env = '', buildEnv = '', apps = [];
    try { env = await fs.readFile(path.join(dir, '.env'), 'utf8'); } catch {}
    try { buildEnv = await fs.readFile(path.join(dir, 'build.env'), 'utf8'); } catch {}
    try { apps = JSON.parse(await fs.readFile(path.join(dir, 'apps.json'), 'utf8')); } catch {}
    products.push({ id: entry.name, env: parseEnv(env), buildEnv: parseEnv(buildEnv), apps: Array.isArray(apps) ? apps : [], hasEnv: Boolean(env), hasExample: files.includes('.env.example'), dirty: false });
  }
  return products;
}
async function writeEnv(id, kind, lines) {
  const file = kind === 'build' ? 'build.env' : '.env';
  const content = lines.map(item => item.key ? `${item.key}=${item.value ?? ''}` : item.line).join('\n');
  const dir = productDir(id);
  const target = path.join(dir, file);
  try { await fs.copyFile(target, `${target}.bak.${Date.now()}`); } catch {}
  const tmp = `${target}.tmp-${process.pid}`;
  await fs.writeFile(tmp, content.endsWith('\n') ? content : `${content}\n`, { mode: kind === 'build' ? 0o644 : 0o600 });
  await fs.rename(tmp, target);
}
function runOperation(id, action) {
  const args = [path.join(root, 'common', 'dc.sh'), id];
  if (action === 'up') args.push('up', '-d');
  if (action === 'down') args.push('down');
  if (action === 'restart') args.push('restart');
  if (action === 'status') args.push('ps');
  if (action === 'logs') args.push('logs', '--tail=80', '--no-color');
  if (!['up', 'down', 'restart', 'status', 'logs'].includes(action)) throw new Error('Ação não permitida.');
  return new Promise((resolve) => {
    const child = spawn('bash', args, { cwd: root, env: { ...process.env }, stdio: ['ignore', 'pipe', 'pipe'] });
    let output = '';
    child.stdout.on('data', data => { output += data.toString(); });
    child.stderr.on('data', data => { output += data.toString(); });
    child.on('error', error => resolve({ ok: false, output: error.message }));
    child.on('close', code => resolve({ ok: code === 0, output, code }));
  });
}

async function snapshot() { return { root, products: await readProducts(), docker: await runVersion('docker', ['--version']), compose: await runVersion('docker', ['compose', 'version']) }; }
function runVersion(program, args) { return new Promise(resolve => { const child = spawn(program, args); let out = ''; child.stdout.on('data', d => out += d); child.stderr.on('data', d => out += d); child.on('close', code => resolve({ ok: code === 0, text: out.trim() })); child.on('error', () => resolve({ ok: false, text: '' })); }); }

function createWindow() {
  const win = new BrowserWindow({ width: 1440, height: 900, minWidth: 1080, minHeight: 680, backgroundColor: '#0f172a', webPreferences: { preload: path.join(__dirname, 'preload.cjs'), contextIsolation: true, nodeIntegration: false, sandbox: true } });
  if (process.env.VITE_DEV_SERVER_URL) win.loadURL(process.env.VITE_DEV_SERVER_URL); else win.loadFile(path.join(root, 'gui', 'dist', 'index.html'));
}
app.whenReady().then(() => {
  ipcMain.handle('snapshot', snapshot);
  ipcMain.handle('save-env', async (_, payload) => { await writeEnv(payload.id, payload.kind, payload.lines); return snapshot(); });
  ipcMain.handle('operation', async (_, { id, action }) => { if (action === 'down') { const choice = await dialog.showMessageBox({ type: 'warning', buttons: ['Cancelar', 'Parar stack'], defaultId: 0, title: 'Confirmar operação', message: `Parar a stack de ${id}?`, detail: 'Volumes não serão removidos.' }); if (choice.response !== 1) return { ok: false, cancelled: true, output: 'Operação cancelada.' }; } return runOperation(id, action); });
  ipcMain.handle('remove-product', async (_, id) => { const choice = await dialog.showMessageBox({ type: 'warning', buttons: ['Cancelar', 'Remover produto'], defaultId: 0, title: 'Remover produto', message: `Remover ${id}?`, detail: 'Somente a pasta products será removida. Volumes Docker permanecem.' }); if (choice.response !== 1) return { ok: false, cancelled: true }; await fs.rm(productDir(id), { recursive: true, force: false }); return snapshot(); });
  createWindow();
  app.on('activate', () => { if (BrowserWindow.getAllWindows().length === 0) createWindow(); });
});
app.on('window-all-closed', () => { if (process.platform !== 'darwin') app.quit(); });
