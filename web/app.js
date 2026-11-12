const START_FEN = 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1';
const FILES = 'abcdefgh';
const spectatorMode = window.location.pathname.endsWith('/match.html');
const worker = new Worker('engine-worker.js');
const boardElement = document.querySelector('#board');
const statusElement = document.querySelector('#game-status');
const stateElement = document.querySelector('#engine-state');
const stateDot = document.querySelector('#state-dot');
const newGameButton = document.querySelector('#new-game');
const undoButton = document.querySelector('#undo');
const colorSelect = document.querySelector('#color-select');
const depthSelect = document.querySelector('#depth-select');
const moveList = document.querySelector('#move-list');

let requestId = 0;
const pending = new Map();
let fen = START_FEN;
let playerSide = 'w';
let selected = null;
let legal = [];
let history = [];
let positions = [START_FEN];
let lastMove = null;
let thinking = false;
let ready = false;
let ended = false;
let liveState = null;

function ask(type, data = {}) {
  return new Promise((resolve, reject) => {
    const id = ++requestId;
    pending.set(id, { resolve, reject });
    worker.postMessage({ id, type, ...data });
  });
}

worker.onmessage = event => {
  const { id, value, error } = event.data;
  const request = pending.get(id);
  if (!request) return;
  pending.delete(id);
  if (error) request.reject(new Error(error)); else request.resolve(value);
};
worker.onerror = event => showFatal(`WebAssembly konnte nicht gestartet werden: ${event.message}`);

function setEngineState(text, mode = '') {
  stateElement.textContent = text;
  stateDot.className = `state-dot ${mode}`;
}

function parseBoard(fenText) {
  const squares = Array(64).fill(null);
  const ranks = fenText.split(' ')[0].split('/');
  ranks.forEach((rankText, row) => {
    let file = 0;
    for (const c of rankText) {
      if (/\d/.test(c)) file += Number(c);
      else squares[(7 - row) * 8 + file++] = c;
    }
  });
  return squares;
}

function squareName(index) { return `${FILES[index % 8]}${Math.floor(index / 8) + 1}`; }
function squareIndex(name) { return FILES.indexOf(name[0]) + (Number(name[1]) - 1) * 8; }

function pieceDrawing(piece) {
  const shapes = {
    K: '<path d="M46 8h8v10h10v8H54v9h-8v-9H36v-8h10zM50 31l-18 9 5 14-13 25h52L63 54l5-14z"/><path d="M31 47l38 2M30 68h40" fill="none" stroke="var(--piece-line)" stroke-width="2.5"/>',
    Q: '<path d="M50 15a5 5 0 1 0 0-10 5 5 0 0 0 0 10ZM25 25a5 5 0 1 0 0-10 5 5 0 0 0 0 10ZM75 25a5 5 0 1 0 0-10 5 5 0 0 0 0 10ZM12 40a5 5 0 1 0 0-10 5 5 0 0 0 0 10ZM88 40a5 5 0 1 0 0-10 5 5 0 0 0 0 10ZM19 31l10 38h42l10-38-20 17-11-30-11 30zM26 75h48v8H26z"/>',
    R: '<path d="M23 13h14v13h9V13h14v13h9V13h14v25l-9 8v25l10 8v8H16v-8l10-8V46l-9-8V13z"/><path d="M30 51h40M30 67h40" fill="none" stroke="var(--piece-line)" stroke-width="3"/>',
    B: '<circle cx="50" cy="17" r="8"/><path d="M50 27c-7 8-17 16-17 26 0 6 5 10 9 12L28 76H72L58 65c5-3 9-7 9-12 0-10-10-18-17-26zM21 81h58v7H21z"/><path d="M55 34L44 52" fill="none" stroke="var(--piece-line)" stroke-width="3"/>',
    N: '<path d="M25 78c5-13 5-21 0-30-5-10 0-17 8-22l-5-13c15 0 26 6 34 16 9 11 12 24 10 39l11 10H21z"/><path d="M38 34l4 1M30 78h49" fill="none" stroke="var(--piece-line)" stroke-width="3"/><circle cx="61" cy="34" r="2.5" fill="var(--piece-line)"/>',
    P: '<circle cx="50" cy="19" r="11"/><path d="M43 32h14c-1 13 4 21 12 29l-5 11H36l-5-11c8-8 13-16 12-29zM24 76h52v9H24z"/>'
  };
  const kind = piece.toUpperCase();
  const side = piece === kind ? 'white' : 'black';
  return `<svg class="piece-art ${side}" viewBox="0 0 100 100" aria-hidden="true">${shapes[kind]}</svg>`;
}

function renderBoard() {
  const squares = parseBoard(fen);
  const [lastFrom, lastTo] = lastMove ? [squareIndex(lastMove.slice(0,2)), squareIndex(lastMove.slice(2,4))] : [-1,-1];
  boardElement.replaceChildren();
  const order = [];
  for (let row = 0; row < 8; row++) {
    const rank = playerSide === 'w' ? 7 - row : row;
    for (let col = 0; col < 8; col++) {
      const file = playerSide === 'w' ? col : 7 - col;
      order.push(rank * 8 + file);
    }
  }
  for (const index of order) {
    const file = index % 8;
    const rank = Math.floor(index / 8);
    const button = document.createElement('button');
    button.type = 'button';
    button.className = `square ${(file + rank) % 2 === 0 ? 'dark' : 'light'}`;
    button.dataset.square = index;
    button.setAttribute('role', 'gridcell');
    button.setAttribute('aria-label', `${squareName(index)}${squares[index] ? `, ${squares[index]}` : ', leer'}`);
    if (index === selected) button.classList.add('selected');
    if (index === lastFrom || index === lastTo) button.classList.add('last-move');
    if (selected !== null && legal.some(m => squareIndex(m.slice(0,2)) === selected && squareIndex(m.slice(2,4)) === index)) {
      button.classList.add(squares[index] ? 'legal-capture' : 'legal-destination');
    }
    if (squares[index]) {
      const piece = document.createElement('span');
      piece.className = 'piece';
      piece.innerHTML = pieceDrawing(squares[index]);
      button.append(piece);
    }
    if ((playerSide === 'w' && rank === 0) || (playerSide === 'b' && rank === 7)) {
      const coordinate = document.createElement('span');
      coordinate.className = `coordinate file-coordinate ${(file + rank) % 2 === 0 ? 'on-dark' : 'on-light'}`;
      coordinate.textContent = FILES[file];
      button.append(coordinate);
    }
    if ((playerSide === 'w' && file === 0) || (playerSide === 'b' && file === 7)) {
      const coordinate = document.createElement('span');
      coordinate.className = `coordinate rank-coordinate ${(file + rank) % 2 === 0 ? 'on-dark' : 'on-light'}`;
      coordinate.textContent = String(rank + 1);
      button.append(coordinate);
    }
    button.addEventListener('click', () => onSquareClick(index));
    boardElement.append(button);
  }
  updatePlayers();
}

function updatePlayers() {
  if (spectatorMode) {
    const gameNo = liveState?.game || 1;
    const valoxWhite = gameNo % 2 === 1;
    const elo = liveState?.stockfishElo || 2000;
    const whiteName = valoxWhite ? 'Valox' : 'Stockfish';
    const blackName = valoxWhite ? 'Stockfish' : 'Valox';
    const whiteCaption = valoxWhite ? 'Weiß · Rust / UCI' : `Weiß · UCI-Elo ${elo}`;
    const blackCaption = valoxWhite ? `Schwarz · UCI-Elo ${elo}` : 'Schwarz · Rust / UCI';
    const topIsValox = blackName === 'Valox';
    const topCard = document.querySelector('#engine-card');
    const bottomCard = document.querySelector('#human-card');
    topCard.querySelector('strong').textContent = blackName;
    topCard.querySelector('small').textContent = blackCaption;
    topCard.querySelector('.avatar').textContent = topIsValox ? 'V' : 'SF';
    bottomCard.querySelector('strong').textContent = whiteName;
    bottomCard.querySelector('small').textContent = whiteCaption;
    bottomCard.querySelector('.avatar').textContent = whiteName === 'Valox' ? 'V' : 'SF';
    const turn = liveState?.turn;
    const clockText = ms => {
      const seconds = Math.ceil(Math.max(0, ms || 0) / 1000);
      return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
    };
    document.querySelector('#human-turn').textContent = `${clockText(liveState?.whiteMs)}${turn === 'w' ? ' · AM ZUG' : ''}`;
    document.querySelector('#engine-turn').textContent = `${clockText(liveState?.blackMs)}${turn === 'b' ? ' · AM ZUG' : ''}`;
    return;
  }
  const engineSide = playerSide === 'w' ? 'b' : 'w';
  document.querySelector('#human-caption').textContent = playerSide === 'w' ? 'Weiß' : 'Schwarz';
  document.querySelector('#engine-caption').textContent = `${engineSide === 'w' ? 'Weiß' : 'Schwarz'} · Rust / WebAssembly`;
  document.querySelector('#human-turn').textContent = !ended && fen.split(' ')[1] === playerSide ? 'AM ZUG' : '';
  document.querySelector('#engine-turn').textContent = thinking ? 'DENKT…' : (!ended && fen.split(' ')[1] === engineSide ? 'AM ZUG' : '');
}

async function onSquareClick(index) {
  if (spectatorMode) return;
  if (!ready || thinking || ended || fen.split(' ')[1] !== playerSide) return;
  if (selected !== null) {
    const choices = legal.filter(m => squareIndex(m.slice(0,2)) === selected && squareIndex(m.slice(2,4)) === index);
    if (choices.length) {
      const move = choices.find(m => m.length === 5 && m[4] === 'q') || choices[0];
      await playMove(move, true);
      return;
    }
  }
  const piece = parseBoard(fen)[index];
  if (piece && (piece === piece.toUpperCase()) === (playerSide === 'w') && legal.some(m => squareIndex(m.slice(0,2)) === index)) {
    selected = index;
    renderBoard();
  } else {
    selected = null;
    renderBoard();
  }
}

async function playMove(move, human) {
  try {
    const nextFen = await ask('apply', { fen, move });
    if (!nextFen) return showFatal('Valox konnte diesen Zug nicht anwenden.');
    fen = nextFen;
    await renderLegal();
    history.push(move);
    positions.push(fen);
    lastMove = move;
    selected = null;
    renderMoves();
    renderBoard();
    ended = await showPositionStatus();
    if (!ended && human && fen.split(' ')[1] !== playerSide) await engineTurn();
    updateControls();
  } catch (error) { showFatal(error.message); }
}

async function engineTurn() {
  if (ended || !ready) return;
  thinking = true;
  setEngineState('Valox denkt…', 'busy');
  statusElement.className = 'status-box';
  statusElement.textContent = 'Valox berechnet seinen Zug.';
  updatePlayers();
  updateControls();
  try {
    const move = await ask('best', { fen, depth: Number(depthSelect.value) });
    if (!move) { ended = await showPositionStatus(); return; }
    await playMove(move, false);
  } catch (error) { showFatal(error.message); }
  finally {
    thinking = false;
    setEngineState('Bereit', 'ready');
    updatePlayers();
    updateControls();
  }
}

async function showPositionStatus() {
  const code = await ask('status', { fen });
  if (code === 1) {
    statusElement.textContent = `${fen.split(' ')[1] === playerSide ? 'Valox' : 'Du'} gewinnt durch Schachmatt.`;
    statusElement.className = 'status-box result'; return true;
  }
  if (code === 2) {
    statusElement.textContent = 'Patt — die Partie ist unentschieden.';
    statusElement.className = 'status-box result'; return true;
  }
  if (code === 3) {
    statusElement.textContent = 'Remis durch die 50-Züge-Regel.';
    statusElement.className = 'status-box result'; return true;
  }
  if (code === 5) {
    statusElement.textContent = 'Remis wegen unzureichenden Materials.';
    statusElement.className = 'status-box result'; return true;
  }
  statusElement.textContent = fen.split(' ')[1] === playerSide ? 'Du bist am Zug.' : 'Valox ist am Zug.';
  statusElement.className = 'status-box';
  return false;
}

async function renderLegal() { legal = await ask('legal', { fen }); }

function renderMoves() {
  moveList.replaceChildren();
  if (!history.length) {
    const empty = document.createElement('li'); empty.className = 'empty-moves'; empty.textContent = 'Die Partie hat noch nicht begonnen.'; moveList.append(empty);
  } else {
    for (let i = 0; i < history.length; i += 2) {
      const row = document.createElement('li'); row.className = 'move-pair';
      const number = document.createElement('span'); number.className = 'move-number'; number.textContent = `${Math.floor(i/2)+1}.`;
      const white = document.createElement('span'); white.textContent = history[i];
      const black = document.createElement('span'); black.textContent = history[i+1] || '…';
      row.append(number, white, black); moveList.append(row);
    }
  }
  document.querySelector('#move-count').textContent = String(history.length);
}

function updateControls() {
  newGameButton.disabled = !ready || thinking;
  undoButton.disabled = !history.length || thinking;
  colorSelect.disabled = thinking;
  depthSelect.disabled = thinking;
}

async function newGame() {
  playerSide = colorSelect.value;
  fen = START_FEN; history = []; positions = [START_FEN]; lastMove = null; selected = null; ended = false;
  await renderLegal(); renderMoves(); renderBoard();
  statusElement.className = 'status-box';
  statusElement.textContent = playerSide === 'w' ? 'Du bist am Zug.' : 'Valox beginnt mit Weiß.';
  updateControls();
  if (playerSide === 'b') await engineTurn();
}

async function undoRound() {
  if (!history.length || thinking) return;
  const plies = Math.min(history.length, 2);
  history.splice(-plies, plies); positions.splice(-plies, plies);
  fen = positions[positions.length - 1]; lastMove = history[history.length - 1] || null; ended = false;
  await renderLegal(); renderMoves(); renderBoard();
  statusElement.className = 'status-box'; statusElement.textContent = 'Zug zurückgenommen.';
  updateControls();
}

function showFatal(message) {
  statusElement.className = 'status-box'; statusElement.textContent = message;
  setEngineState('Fehler', '');
}

async function pollLiveMatch() {
  try {
    const response = await fetch('/api/state', { cache: 'no-store' });
    if (!response.ok) throw new Error(`Match-Server antwortet nicht (${response.status}).`);
    const oldFen = fen;
    const oldMoves = history.join(' ');
    liveState = await response.json();
    fen = liveState.fen || START_FEN;
    history = liveState.moves || [];
    lastMove = history.at(-1) || null;
    ended = liveState.status === 'finished' || liveState.status === 'stopped' || liveState.status === 'error';
    document.querySelector('.eyebrow').textContent = 'ENGINE-MATCH · LIVE';
    document.querySelector('h1').textContent = 'Valox gegen Stockfish';
    const stockfishElo = liveState.stockfishElo || 2000;
    document.querySelector('.intro').textContent = `Einzelpartie gegen Stockfish mit UCI-Elo ${stockfishElo}.`;
    document.querySelector('.version-pill').textContent = `Spiel ${liveState.game}/${liveState.total}`;
    const scoreCard = document.querySelector('#match-score');
    scoreCard.hidden = false;
    const valoxPoints = Number(liveState.score || 0);
    const stockfishPoints = (liveState.results || []).length - valoxPoints;
    document.querySelector('#score-points').textContent = `${valoxPoints} – ${stockfishPoints}`;
    document.querySelector('#score-bar').style.width = `${liveState.results?.length ? (valoxPoints / liveState.results.length) * 100 : 50}%`;
    const wins = liveState.results?.filter(result => result.valox === 1).length || 0;
    const draws = liveState.results?.filter(result => result.valox === 0.5).length || 0;
    const losses = liveState.results?.filter(result => result.valox === 0).length || 0;
    document.querySelector('#score-record').textContent = `${wins} Siege · ${draws} Remis · ${losses} Niederlagen`;
    document.querySelector('#score-progress').textContent = `${Math.min(liveState.game, liveState.total)}/${liveState.total} Partien`;
    const recent = document.querySelector('#recent-results');
    recent.replaceChildren();
    for (const result of (liveState.results || []).slice(-5)) {
      const item = document.createElement('span');
      item.className = `result-chip ${result.valox === 1 ? 'win' : result.valox === 0 ? 'loss' : 'draw'}`;
      item.textContent = `P${result.game} ${result.valox === 1 ? 'V' : result.valox === 0 ? 'S' : '='}`;
      item.title = result.result;
      recent.append(item);
    }
    document.querySelector('.moves-heading h2').textContent = 'Partie-Züge';
    const initialClockMs = liveState.initialClockMs || 15_000;
    const timeLabel = initialClockMs % 60_000 === 0 ? `${initialClockMs / 60_000} Min.` : `${Math.round(initialClockMs / 1000)} Sek.`;
    const gameLabel = liveState.total === 1 ? '1 Partie' : `${liveState.total} Partien`;
    document.querySelector('.fine-print').textContent = `${gameLabel} · ${timeLabel} pro Engine · Stockfish nach UCI auf ${stockfishElo} eingestellt. UCI-Elo ist eine Spielstärke-Einstellung, keine gemessene Wertungszahl.`;
    document.querySelector('#game-status').textContent = liveState.message || 'Warte auf die Engines…';
    document.querySelector('#game-status').className = `status-box${ended && liveState.status === 'finished' ? ' result' : ''}`;
    setEngineState(liveState.status === 'playing' ? 'PARTIE LÄUFT' : liveState.status === 'finished' ? 'MATCH BEENDET' : liveState.status === 'error' ? 'FEHLER' : 'STARTET…', liveState.status === 'playing' ? 'busy' : liveState.status === 'finished' ? 'ready' : '');
    if (oldMoves !== history.join(' ')) renderMoves();
    if (oldFen !== fen) renderBoard();
    document.querySelector('#move-count').textContent = liveState.scoreText || '0–0–0';
    updatePlayers();
  } catch (error) { showFatal(error.message); }
}

newGameButton.addEventListener('click', newGame);
undoButton.addEventListener('click', undoRound);
colorSelect.addEventListener('change', newGame);

(async () => {
  renderBoard();
  try {
    await ask('init');
    ready = true;
    if (spectatorMode) {
      document.body.classList.add('spectator');
      colorSelect.hidden = true; depthSelect.hidden = true;
      document.querySelectorAll('.field-label').forEach(el => el.hidden = true);
      newGameButton.hidden = true; undoButton.hidden = true;
      document.querySelector('.eyebrow').textContent = 'ENGINE-MATCH · LIVE';
      document.querySelector('h1').textContent = 'Valox gegen Stockfish';
      await pollLiveMatch();
      setInterval(pollLiveMatch, 500);
      return;
    }
    setEngineState('Bereit', 'ready');
    await newGame();
  } catch (error) { showFatal(error.message); }
})();
