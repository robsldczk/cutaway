// A stand-in for the Epidemic Sound Partner API for the UI tests: 40 sound effects per
// search term, previews and downloads as short WAVs. Any Bearer key is accepted.
import http from 'node:http';

const PORT = 7359;
const base = `http://127.0.0.1:${PORT}`;

function wav(id) {
  const sr = 48000;
  const n = sr / 2;
  const b = Buffer.alloc(44 + n * 2);
  b.write('RIFF', 0);
  b.writeUInt32LE(36 + n * 2, 4);
  b.write('WAVEfmt ', 8);
  b.writeUInt32LE(16, 16);
  b.writeUInt16LE(1, 20);
  b.writeUInt16LE(1, 22);
  b.writeUInt32LE(sr, 24);
  b.writeUInt32LE(sr * 2, 28);
  b.writeUInt16LE(2, 32);
  b.writeUInt16LE(16, 34);
  b.write('data', 36);
  b.writeUInt32LE(n * 2, 40);
  for (let i = 0; i < n; i++) b.writeInt16LE(Math.round(6000 * Math.sin((i * (200 + id) * 2 * Math.PI) / sr)), 44 + i * 2);
  return b;
}

http
  .createServer((req, res) => {
    const url = new URL(req.url ?? '/', base);
    const json = (v, code = 200) => {
      res.writeHead(code, { 'content-type': 'application/json' });
      res.end(JSON.stringify(v));
    };
    if (url.pathname === '/health') return json({ ok: true });
    const file = url.pathname.match(/^\/files\/(\d+)\.wav$/);
    if (file) {
      res.writeHead(200, { 'content-type': 'audio/wav' });
      return res.end(wav(+file[1]));
    }
    if (!(req.headers.authorization ?? '').startsWith('Bearer ')) return json({ error: 'unauthorized' }, 401);
    if (url.pathname === '/v0/sound-effects/search') {
      const term = url.searchParams.get('term') ?? '';
      const limit = +(url.searchParams.get('limit') ?? 10);
      const offset = +(url.searchParams.get('offset') ?? 0);
      const all = Array.from({ length: 40 }, (_, i) => ({ id: String(500 + i), title: `${term} sound ${500 + i}`, length: 2 }));
      return json({ soundEffects: all.slice(offset, offset + limit), pagination: {}, links: {} });
    }
    const m = url.pathname.match(/^\/v0\/sound-effects\/(\d+)\/(preview|download)$/);
    if (m) return json({ url: `${base}/files/${m[1]}.wav`, expires: 'x' });
    json({}, 404);
  })
  .listen(PORT, '127.0.0.1');
