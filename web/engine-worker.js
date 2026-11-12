let instance;

async function initialize() {
  if (instance) return instance;
  const response = await fetch('valox.wasm');
  if (!response.ok) throw new Error(`WASM-Datei konnte nicht geladen werden (${response.status}).`);
  const bytes = await response.arrayBuffer();
  const result = await WebAssembly.instantiate(bytes, { env: { valox_now_ms: () => performance.now() } });
  instance = result.instance;
  return instance;
}

function encode(text) { return new TextEncoder().encode(text); }
function decode(bytes) { return new TextDecoder().decode(bytes); }

function withBuffers(api, inputs, outputCapacity) {
  const wasm = instance.exports;
  const inBytes = inputs.map(encode);
  const inPtrs = inBytes.map(bytes => wasm.valox_alloc(bytes.length) >>> 0);
  const outPtr = wasm.valox_alloc(outputCapacity) >>> 0;
  for (let i = 0; i < inBytes.length; i++) new Uint8Array(wasm.memory.buffer, inPtrs[i], inBytes[i].length).set(inBytes[i]);
  const resultLength = api(wasm, inPtrs, inBytes, outPtr, outputCapacity);
  const result = resultLength ? decode(new Uint8Array(wasm.memory.buffer, outPtr, resultLength)) : '';
  for (let i = 0; i < inPtrs.length; i++) wasm.valox_dealloc(inPtrs[i], inBytes[i].length);
  wasm.valox_dealloc(outPtr, outputCapacity);
  return result;
}

function legalMoves(fen) {
  return withBuffers((wasm, ptrs, bytes, out, cap) => wasm.valox_legal_moves(ptrs[0], bytes[0].length, out, cap), [fen], 2048).split(/\s+/).filter(Boolean);
}

function applyMove(fen, move) {
  return withBuffers((wasm, ptrs, bytes, out, cap) => wasm.valox_apply_move(ptrs[0], bytes[0].length, ptrs[1], bytes[1].length, out, cap), [fen, move], 128);
}

function status(fen) {
  const wasm = instance.exports;
  const bytes = encode(fen);
  const pointer = wasm.valox_alloc(bytes.length) >>> 0;
  new Uint8Array(wasm.memory.buffer, pointer, bytes.length).set(bytes);
  const result = wasm.valox_status(pointer, bytes.length);
  wasm.valox_dealloc(pointer, bytes.length);
  return result;
}

function bestMove(fen, depth) {
  return withBuffers((wasm, ptrs, bytes, out, cap) => wasm.valox_best_move(ptrs[0], bytes[0].length, depth, out, cap), [fen], 8);
}

self.onmessage = async event => {
  const { id, type, fen, move, depth } = event.data;
  try {
    await initialize();
    let value;
    if (type === 'legal') value = legalMoves(fen);
    else if (type === 'apply') value = applyMove(fen, move);
    else if (type === 'status') value = Number(status(fen));
    else if (type === 'best') value = bestMove(fen, depth);
    else if (type === 'init') value = true;
    else throw new Error('Unbekannter Engine-Befehl.');
    self.postMessage({ id, value });
  } catch (error) {
    self.postMessage({ id, error: error.message || String(error) });
  }
};
