// Serves the game and proxies the law clerk. The simulation itself runs in the
// player's browser; this process holds only the API key.
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Sim } from '../web/sim.js';
import { draft, available, MODEL } from './drafter.js';

const here = path.dirname(fileURLToPath(import.meta.url));
const WEB = path.resolve(here, '../web');
const WASM = path.join(WEB, 'pkg/society.wasm');
const PORT = Number(process.env.PORT || 5173);

const TYPES = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8', '.wasm': 'application/wasm', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png', '.ico': 'image/x-icon' };

// The clerk's manual comes from the simulation itself, so the two cannot drift apart.
let manual = '';
async function loadManual() {
  if (!fs.existsSync(WASM)) return false;
  const sim = await Sim.load(fs.readFileSync(WASM));
  manual = sim.call({ cmd: 'manual' }).manual || '';
  return true;
}

function send(res, status, body, type = 'application/json') {
  const data = typeof body === 'string' || Buffer.isBuffer(body) ? body : JSON.stringify(body);
  res.writeHead(status, { 'Content-Type': type, 'Cache-Control': 'no-store' });
  res.end(data);
}

function readJson(req, limit = 4_000_000) {
  return new Promise((resolve, reject) => {
    let size = 0;
    const chunks = [];
    req.on('data', (c) => {
      size += c.length;
      if (size > limit) { reject(Object.assign(new Error('request too large'), { status: 413 })); req.destroy(); return; }
      chunks.push(c);
    });
    req.on('end', () => {
      try { resolve(JSON.parse(Buffer.concat(chunks).toString('utf8') || '{}')); }
      catch { reject(Object.assign(new Error('bad JSON'), { status: 400 })); }
    });
    req.on('error', reject);
  });
}

function serveStatic(req, res, pathname) {
  const rel = pathname === '/' ? 'index.html' : decodeURIComponent(pathname).replace(/^\/+/, '');
  const file = path.resolve(WEB, rel);
  if (file !== WEB && !file.startsWith(WEB + path.sep)) return send(res, 403, 'Forbidden', 'text/plain');
  fs.readFile(file, (err, data) => {
    if (err) return send(res, 404, 'Not found', 'text/plain');
    send(res, 200, data, TYPES[path.extname(file)] || 'application/octet-stream');
  });
}

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, 'http://localhost');
  try {
    if (url.pathname === '/api/status') {
      return send(res, 200, { clerk: available(), model: MODEL, manual: manual.length > 0 });
    }
    if (url.pathname === '/api/draft' && req.method === 'POST') {
      if (!available()) return send(res, 503, { error: 'The law clerk needs an API key. Start the server with ANTHROPIC_API_KEY set.' });
      if (!manual) return send(res, 503, { error: 'The simulation has not been built. Run "npm run build" and restart the server.' });
      const body = await readJson(req);
      if (!Array.isArray(body.history)) return send(res, 400, { error: 'missing "history"' });
      const out = await draft({ history: body.history, snapshot: typeof body.snapshot === 'string' ? body.snapshot : '', manual });
      return send(res, 200, out);
    }
    if (req.method === 'GET') return serveStatic(req, res, url.pathname);
    send(res, 405, { error: 'method not allowed' });
  } catch (err) {
    if (!err.status) console.error(err);
    send(res, err.status || 500, { error: err.status ? err.message : 'Something went wrong on the server.' });
  }
});

loadManual()
  .then((built) => {
    if (!built) console.warn('web/pkg/society.wasm is missing. Run "npm run build" first.');
    server.listen(PORT, () => {
      console.log(`Society Simulation at http://localhost:${PORT}`);
      console.log(available() ? `Law clerk ready (${MODEL}).` : 'No ANTHROPIC_API_KEY set: the game runs, but new laws cannot be drafted from English.');
    });
  })
  .catch((err) => { console.error('Could not load the simulation:', err.message); process.exit(1); });
