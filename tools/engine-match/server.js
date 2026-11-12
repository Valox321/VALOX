const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const { spawn } = require('node:child_process');

const root = path.resolve(__dirname, '../..');
const web = path.join(root, 'web');
const valoxExe = path.join(root, 'build', 'valox.exe');
const stockfishExe = path.join(__dirname, 'stockfish', 'stockfish', 'stockfish-windows-x86-64-universal.exe');
const syzygyPath = path.join(root, 'syzygy', '3-4-5-wdl');
const startFen = 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1';
const openings = [
  ['e2e4','e7e5','g1f3','b8c6','f1b5','a7a6','b5a4','g8f6'],
  ['d2d4','d7d5','c2c4','e7e6','b1c3','g8f6','g1f3','f8e7'],
  ['e2e4','c7c5','g1f3','d7d6','d2d4','c5d4','f3d4','g8f6'],
  ['c2c4','e7e5','b1c3','g8f6','g2g3','d7d5','c4d5','f6d5'],
  ['e2e4','c7c6','d2d4','d7d5','b1c3','d5e4','c3e4','c8f5'],
  ['d2d4','g8f6','c2c4','g7g6','b1c3','f8g7','e2e4','d7d6'],
  ['e2e4','e7e6','d2d4','d7d5','b1c3','g8f6','c1g5','f8e7'],
  ['g1f3','d7d5','g2g3','g8f6','f1g2','e7e6','e1g1','f8e7'],
  ['c2c4','c7c5','g1f3','g8f6','b1c3','e7e6','g2g3','d7d5'],
  ['e2e4','d7d5','e4d5','d8d5','b1c3','d5a5','d2d4','g8f6'],
  ['d2d4','g8f6','g1f3','e7e6','c2c4','b7b6','g2g3','c8a6'],
  ['e2e4','c7c5','b1c3','b8c6','f2f4','g7g6','g1f3','f8g7'],
  ['d2d4','d7d5','c1f4','g8f6','e2e3','e7e6','g1f3','c7c5'],
  ['b2b3','d7d5','c1b2','g8f6','e2e3','e7e6','g1f3','f8e7'],
  ['g2g3','d7d5','f1g2','e7e5','c2c4','d5d4','g1f3','b8c6'],
  ['e2e4','e7e5','b1c3','g8f6','f2f4','d7d5','e4d5','f6d5'],
  ['e2e4','e7e5','f2f4','e5f4','g1f3','g7g5','h2h4','g5g4'],
  ['e2e4','e7e5','g1f3','b8c6','d2d4','e5d4','f3d4','g8f6'],
  ['c2c4','e7e6','b1c3','d7d5','d2d4','g8f6','g1f3','f8e7'],
  ['d2d4','g8f6','c2c4','e7e6','g1f3','d7d5','b1c3','f8e7'],
];
// Choose a fresh opening pair for each new server session so a restarted
// match is visibly distinct while still giving both engines the same setup.
const openingOffset = Math.floor(Math.random() * openings.length);
const requestedGames = Number.parseInt(process.env.VALOX_MATCH_GAMES || '', 10);
const totalGames = Number.isFinite(requestedGames) && requestedGames > 0 ? Math.min(requestedGames, openings.length * 2) : openings.length * 2;
const requestedClockMs = Number.parseInt(process.env.VALOX_MATCH_TIME_MS || '', 10);
const initialClockMs = Number.isFinite(requestedClockMs) && requestedClockMs > 0 ? Math.min(requestedClockMs, 3_600_000) : 15_000;
const requestedElo = Number.parseInt(process.env.VALOX_STOCKFISH_ELO || '2000', 10);
const stockfishElo = Number.isFinite(requestedElo) ? Math.max(1320, Math.min(requestedElo, 3190)) : 2000;
const incrementMs = 0;

let gameState = { status: 'starting', game: 0, total: totalGames, initialClockMs, whiteMs: initialClockMs, blackMs: initialClockMs, thinkingSide: null, thinkingStartedAt: null, stockfishElo, moves: [], fen: startFen, score: 0, results: [], message: 'Engines werden gestartet…' };
let stopRequested = false;
let wasm;

function wasmTextCall(name, inputs, capacity) {
  const enc = new TextEncoder();
  const bytes = inputs.map(s => enc.encode(s));
  const ptrs = bytes.map(b => wasm.valox_alloc(b.length) >>> 0);
  const out = wasm.valox_alloc(capacity) >>> 0;
  bytes.forEach((b, i) => new Uint8Array(wasm.memory.buffer, ptrs[i], b.length).set(b));
  const n = name === 'apply'
    ? wasm.valox_apply_move(ptrs[0], bytes[0].length, ptrs[1], bytes[1].length, out, capacity)
    : wasm.valox_legal_moves(ptrs[0], bytes[0].length, out, capacity);
  const value = n ? new TextDecoder().decode(new Uint8Array(wasm.memory.buffer, out, n)) : '';
  bytes.forEach((b, i) => wasm.valox_dealloc(ptrs[i], b.length));
  wasm.valox_dealloc(out, capacity);
  return value;
}

function wasmStatus(fen) {
  const b = new TextEncoder().encode(fen);
  const p = wasm.valox_alloc(b.length) >>> 0;
  new Uint8Array(wasm.memory.buffer, p, b.length).set(b);
  const n = wasm.valox_status(p, b.length);
  wasm.valox_dealloc(p, b.length);
  return n;
}

class UciEngine {
  constructor(name, exe, options) {
    this.name = name; this.exe = exe; this.options = options; this.queue = []; this.waiters = [];
    this.buffer = ''; this.proc = null;
  }
  send(text) { this.proc.stdin.write(text + '\n'); }
  onLine(line) {
    this.queue.push(line);
    for (const wake of this.waiters.splice(0)) wake();
  }
  async until(predicate, timeoutMs = 20_000) {
    const end = Date.now() + timeoutMs;
    while (true) {
      const i = this.queue.findIndex(predicate);
      if (i >= 0) return this.queue.splice(i, 1)[0];
      const left = end - Date.now();
      if (left <= 0) throw new Error(`${this.name}: UCI-Antwort blieb aus.`);
      await new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error(`${this.name}: Zeitüberschreitung beim Warten auf UCI.`)), left);
        this.waiters.push(() => { clearTimeout(timer); resolve(); });
      });
    }
  }
  async start() {
    if (!fs.existsSync(this.exe)) throw new Error(`Engine-Datei fehlt: ${this.exe}`);
    this.proc = spawn(this.exe, [], { cwd: path.dirname(this.exe), stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
    this.proc.stdout.on('data', chunk => {
      this.buffer += chunk.toString();
      const lines = this.buffer.split(/\r?\n/); this.buffer = lines.pop();
      for (const line of lines) this.onLine(line.trim());
    });
    this.proc.stderr.on('data', chunk => { this.lastError = chunk.toString().trim(); });
    this.proc.on('error', error => this.onLine(`__error__ ${error.message}`));
    this.send('uci');
    await this.until(line => line === 'uciok');
    for (const [name, value] of Object.entries(this.options)) this.send(`setoption name ${name} value ${value}`);
    this.send('isready');
    await this.until(line => line === 'readyok');
  }
  async newGame() {
    this.send('ucinewgame');
    this.send('isready');
    await this.until(line => line === 'readyok');
  }
  async bestMove(moves, whiteTime, blackTime) {
    this.send('position startpos' + (moves.length ? ` moves ${moves.join(' ')}` : ''));
    this.send(`go wtime ${Math.max(1, Math.floor(whiteTime))} btime ${Math.max(1, Math.floor(blackTime))} winc ${incrementMs} binc ${incrementMs}`);
    const line = await this.until(x => x.startsWith('bestmove '), 180_000);
    return line.split(/\s+/)[1];
  }
  close() { if (this.proc && this.proc.exitCode === null) { this.send('quit'); setTimeout(() => this.proc.kill(), 1500).unref(); } }
}

function currentScoreText() {
  const w = gameState.results.filter(x => x.valox === 1).length;
  const d = gameState.results.filter(x => x.valox === 0.5).length;
  const l = gameState.results.filter(x => x.valox === 0).length;
  return `${w}-${d}-${l} (S-U-N)`;
}

async function playMatch() {
  try {
    const bytes = fs.readFileSync(path.join(web, 'valox.wasm'));
    const result = await WebAssembly.instantiate(bytes, { env: { valox_now_ms: () => performance.now() } });
    wasm = result.instance.exports;
    const valox = new UciEngine('Valox', valoxExe, { Hash: 64, SyzygyPath: syzygyPath });
    const stockfish = new UciEngine('Stockfish', stockfishExe, { Threads: 1, Hash: 64, SyzygyPath: syzygyPath, UCI_LimitStrength: 'true', UCI_Elo: stockfishElo });
    await Promise.all([valox.start(), stockfish.start()]);
    gameState.status = 'playing'; gameState.message = 'Match läuft';
    for (let game = 1; game <= totalGames && !stopRequested; game++) {
      await Promise.all([valox.newGame(), stockfish.newGame()]);
      gameState.game = game; gameState.moves = []; gameState.fen = startFen;
      const opening = openings[(Math.floor((game - 1) / 2) + openingOffset) % openings.length];
      let moves = [...opening];
      let fen = startFen;
      for (const move of moves) fen = wasmTextCall('apply', [fen, move], 128);
      let whiteMs = initialClockMs, blackMs = initialClockMs;
      gameState.whiteMs = whiteMs; gameState.blackMs = blackMs;
      let winner = null, finishReason = 'Unentschieden';
      let repeat = new Map();
      for (let ply = 0; ply < 400; ply++) {
        if (stopRequested) break;
        const positionKey = fen.split(' ').slice(0, 4).join(' ');
        repeat.set(positionKey, (repeat.get(positionKey) || 0) + 1);
        // Threefold repetition is claimable, not automatic. UCI has no standard
        // draw-claim command, so only stop automatically at the fivefold rule.
        if (repeat.get(positionKey) >= 5) { finishReason = 'Fünffache Stellungswiederholung'; break; }
        const status = wasmStatus(fen);
        if (status === 1) { winner = fen.split(' ')[1] === 'w' ? 'b' : 'w'; finishReason = 'Schachmatt'; break; }
        if (status === 2 || status === 3 || status === 5) { finishReason = status === 2 ? 'Patt' : status === 3 ? '50-Züge-Regel' : 'unzureichendes Material'; break; }
        const side = fen.split(' ')[1];
        const engine = (side === 'w' ? (game % 2 === 1 ? valox : stockfish) : (game % 2 === 1 ? stockfish : valox));
        gameState.turn = side; gameState.fen = fen; gameState.moves = moves.slice();
        gameState.whiteMs = whiteMs; gameState.blackMs = blackMs;
        gameState.thinkingSide = side; gameState.thinkingStartedAt = Date.now();
        gameState.message = `Partie ${game}/${totalGames} · ${engine.name} denkt …`;
        const started = Date.now();
        const move = await engine.bestMove(moves, whiteMs, blackMs);
        const elapsed = Date.now() - started;
        if (side === 'w') whiteMs = Math.min(initialClockMs + ply * incrementMs, whiteMs - elapsed + incrementMs);
        else blackMs = Math.min(initialClockMs + ply * incrementMs, blackMs - elapsed + incrementMs);
        gameState.whiteMs = whiteMs; gameState.blackMs = blackMs;
        gameState.thinkingSide = null; gameState.thinkingStartedAt = null;
        if (!move || move === '0000' || move === '(none)') {
          const endStatus = wasmStatus(fen);
          if (endStatus === 1) { winner = side === 'w' ? 'b' : 'w'; finishReason = 'Schachmatt'; }
          else finishReason = 'Keine legalen Züge';
          break;
        }
        if ((side === 'w' ? whiteMs : blackMs) <= 0) { winner = side === 'w' ? 'b' : 'w'; finishReason = 'Zeitüberschreitung'; break; }
        const next = wasmTextCall('apply', [fen, move], 128);
        if (!next) throw new Error(`${engine.name} spielte den illegalen Zug ${move} in ${fen}`);
        moves.push(move); fen = next;
        gameState.fen = fen; gameState.moves = moves.slice();
      }
      if (stopRequested) break;
      const whiteIsValox = game % 2 === 1;
      const valoxScore = winner === null ? 0.5 : ((winner === 'w') === whiteIsValox ? 1 : 0);
      const resultText = valoxScore === 1 ? '1-0 Valox gewinnt' : valoxScore === 0 ? '0-1 Stockfish gewinnt' : '½-½ Remis';
      gameState.results.push({ game, valox: valoxScore, result: resultText, reason: finishReason, moves: moves.slice() });
      gameState.score = gameState.results.reduce((sum, r) => sum + r.valox, 0);
      gameState.message = `Partie ${game}/${totalGames} beendet: ${resultText} (${finishReason}) · Gesamt ${currentScoreText()}`;
      gameState.status = game === totalGames ? 'finished' : 'playing';
      await new Promise(resolve => setTimeout(resolve, 2200));
    }
    valox.close(); stockfish.close();
    if (stopRequested) { gameState.status = 'stopped'; gameState.message = `Match abgebrochen · Gesamt ${currentScoreText()}`; }
    else { gameState.status = 'finished'; gameState.message = `Match beendet · Valox ${currentScoreText()} gegen Stockfish · UCI-Elo-Einstellung ${stockfishElo}`; }
  } catch (error) {
    gameState.status = 'error'; gameState.message = `Fehler im Match: ${error.message}`;
    console.error(error);
  }
}

const mime = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8', '.wasm': 'application/wasm' };
const server = http.createServer((req, res) => {
  const url = new URL(req.url, 'http://127.0.0.1');
  if (url.pathname === '/api/state') {
    res.writeHead(200, { 'Content-Type': 'application/json; charset=utf-8', 'Cache-Control': 'no-store', 'Access-Control-Allow-Origin': '*' });
    const state = { ...gameState };
    if (state.thinkingSide && state.thinkingStartedAt) {
      const sideClock = state.thinkingSide === 'w' ? 'whiteMs' : 'blackMs';
      state[sideClock] = Math.max(0, state[sideClock] - (Date.now() - state.thinkingStartedAt));
    }
    return res.end(JSON.stringify({ ...state, scoreText: currentScoreText() }));
  }
  if (url.pathname === '/api/stop' && req.method === 'POST') { stopRequested = true; res.writeHead(200); return res.end('ok'); }
  let file = path.resolve(web, `.${decodeURIComponent(url.pathname === '/' ? '/match.html' : url.pathname)}`);
  if (!file.startsWith(web + path.sep) && file !== path.join(web, 'match.html')) { res.writeHead(403); return res.end(); }
  fs.readFile(file, (err, data) => {
    if (err) { res.writeHead(404); return res.end('Not found'); }
    res.writeHead(200, { 'Content-Type': mime[path.extname(file)] || 'application/octet-stream', 'Cache-Control': 'no-store' });
    res.end(data);
  });
});
server.listen(8766, '127.0.0.1', () => { console.log('Valox match browser at http://127.0.0.1:8766/match.html'); playMatch(); });
process.on('SIGINT', () => { stopRequested = true; server.close(); });
