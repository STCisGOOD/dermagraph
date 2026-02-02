

import express from 'express';
import cors from 'cors';
import fetch from 'node-fetch';

const app = express();
const PORT = 3001;

const DAEMON_URL = process.env.DAEMON_URL || 'http://localhost:31415';

app.use(cors({
  origin: ['http://localhost:5173', 'http://localhost:5174', 'http://127.0.0.1:5173', 'http://127.0.0.1:5174'],
  methods: ['GET', 'POST', 'OPTIONS'],
  allowedHeaders: ['Content-Type'],
}));
app.use(express.json());

app.use((req, res, next) => {
  const timestamp = new Date().toISOString();
  console.log(`[${timestamp}] ${req.method} ${req.path}`);
  next();
});

app.get('/health', (req, res) => {
  res.json({ status: 'ok', bridge: 'running', daemon_url: DAEMON_URL });
});

app.get('/status', async (req, res) => {
  try {
    const response = await fetch(`${DAEMON_URL}/status`);
    const data = await response.json();
    console.log('[STATUS] Daemon response:', data);
    res.json(data);
  } catch (error) {
    console.error('[STATUS] Error:', error.message);
    res.status(503).json({
      success: false,
      error: 'Cannot connect to daemon',
      details: error.message,
    });
  }
});

app.post('/register', async (req, res) => {
  console.log('[REGISTER] Starting fingerprint registration...');

  try {
    const response = await fetch(`${DAEMON_URL}/register`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(req.body),
    });

    const data = await response.json();
    console.log('[REGISTER] Result:', data.success ? 'SUCCESS' : 'FAILED');

    if (data.data?.commitment) {
      console.log('[REGISTER] Commitment:', data.data.commitment.slice(0, 20) + '...');
    }

    res.json(data);
  } catch (error) {
    console.error('[REGISTER] Error:', error.message);
    res.status(500).json({
      success: false,
      error: 'Registration failed',
      details: error.message,
    });
  }
});

app.post('/authenticate', async (req, res) => {
  const { scope } = req.body;
  console.log(`[AUTH] Authenticating for scope: "${scope}"`);

  if (!scope) {
    return res.status(400).json({
      success: false,
      error: 'Scope is required',
    });
  }

  try {
    const response = await fetch(`${DAEMON_URL}/authenticate`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ scope }),
    });

    const data = await response.json();

    if (data.success) {
      console.log('[AUTH] SUCCESS - Nullifier:', data.data?.nullifier?.slice(0, 20) + '...');
    } else {
      console.log('[AUTH] FAILED:', data.error);
    }

    res.json(data);
  } catch (error) {
    console.error('[AUTH] Error:', error.message);
    res.status(500).json({
      success: false,
      error: 'Authentication failed',
      details: error.message,
    });
  }
});

app.post('/verify', async (req, res) => {
  console.log('[VERIFY] Verifying nullifier...');

  try {
    const response = await fetch(`${DAEMON_URL}/verify`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(req.body),
    });

    const data = await response.json();
    console.log('[VERIFY] Result:', data.success ? 'VALID' : 'INVALID');
    res.json(data);
  } catch (error) {
    console.error('[VERIFY] Error:', error.message);
    res.status(500).json({
      success: false,
      error: 'Verification failed',
      details: error.message,
    });
  }
});

app.get('/enroll-fingers-stream', async (req, res) => {
  const { scope, passphrase } = req.query;
  console.log('[ENROLL-STREAM] Starting SSE enrollment stream...');

  res.setHeader('Content-Type', 'text/event-stream');
  res.setHeader('Cache-Control', 'no-cache');
  res.setHeader('Connection', 'keep-alive');
  res.setHeader('Access-Control-Allow-Origin', req.headers.origin || '*');
  res.flushHeaders();

  try {
    const url = new URL(`${DAEMON_URL}/enroll-fingers-stream`);
    url.searchParams.set('scope', scope || 'default');
    if (passphrase) url.searchParams.set('passphrase', passphrase);

    const response = await fetch(url.toString(), {
      headers: { 'Accept': 'text/event-stream' },
    });

    if (!response.ok) {
      res.write(`data: ${JSON.stringify({ event: "error", data: { message: "Daemon connection failed" } })}\n\n`);
      res.end();
      return;
    }

    response.body.on('data', (chunk) => {
      const text = chunk.toString();
      console.log('[ENROLL-STREAM] Forwarding:', text.trim().substring(0, 80));
      res.write(text);
      if (res.flush) res.flush();
    });

    response.body.on('end', () => {
      console.log('[ENROLL-STREAM] Stream complete');
      res.end();
    });

    response.body.on('error', (err) => {
      console.error('[ENROLL-STREAM] Stream error:', err.message);
      res.end();
    });

    req.on('close', () => {
      console.log('[ENROLL-STREAM] Client disconnected');
      response.body.destroy();
    });
  } catch (error) {
    console.error('[ENROLL-STREAM] Error:', error.message);
    res.write(`data: ${JSON.stringify({ event: "error", data: { message: error.message } })}\n\n`);
    res.end();
  }
});

app.post('/enroll-fingers', async (req, res) => {
  console.log('[ENROLL-FINGERS] Starting 3-finger enrollment...');

  try {
    const response = await fetch(`${DAEMON_URL}/enroll-fingers`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(req.body),
    });

    const data = await response.json();
    console.log('[ENROLL-FINGERS] Result:', data.success ? 'SUCCESS' : 'FAILED');

    if (data.data?.commitment) {
      console.log('[ENROLL-FINGERS] Commitment:', data.data.commitment.slice(0, 20) + '...');
    }

    res.json(data);
  } catch (error) {
    console.error('[ENROLL-FINGERS] Error:', error.message);
    res.status(500).json({
      success: false,
      error: 'Multi-finger enrollment failed',
      details: error.message,
    });
  }
});

app.post('/prove-person', async (req, res) => {
  const { scope } = req.body;
  console.log(`[PROVE-PERSON] Generating Noir ZK proof for scope: "${scope}"`);

  try {
    const response = await fetch(`${DAEMON_URL}/prove-person`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ scope }),
    });

    const data = await response.json();

    if (data.success) {
      console.log('[PROVE-PERSON] SUCCESS - Proof generated');
      console.log('[PROVE-PERSON] Nullifier:', data.data?.nullifier?.slice(0, 20) + '...');
    } else {
      console.log('[PROVE-PERSON] FAILED:', data.error);
    }

    res.json(data);
  } catch (error) {
    console.error('[PROVE-PERSON] Error:', error.message);
    res.status(500).json({
      success: false,
      error: 'Proof generation failed',
      details: error.message,
    });
  }
});

app.post('/verify-finger', async (req, res) => {
  const { scope } = req.body;
  console.log(`[VERIFY-FINGER] Verifying with scope: "${scope}"`);

  if (!scope) {
    return res.status(400).json({
      success: false,
      error: 'Scope is required',
    });
  }

  try {
    const response = await fetch(`${DAEMON_URL}/verify-finger`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ scope }),
    });

    const data = await response.json();

    if (data.success) {
      console.log('[VERIFY-FINGER] SUCCESS - Nullifier:', data.data?.nullifier?.slice(0, 20) + '...');
    } else {
      console.log('[VERIFY-FINGER] FAILED:', data.error);
    }

    res.json(data);
  } catch (error) {
    console.error('[VERIFY-FINGER] Error:', error.message);
    res.status(500).json({
      success: false,
      error: 'Finger verification failed',
      details: error.message,
    });
  }
});

const HOST = process.env.HOST || '0.0.0.0';
app.listen(PORT, HOST, () => {
  console.log('');
  console.log('╔══════════════════════════════════════════════════════════════╗');
  console.log('║           STH Bridge Server - Biometric DAO Voting           ║');
  console.log('╠══════════════════════════════════════════════════════════════╣');
  console.log(`║  Bridge running on:  http://localhost:${PORT}                   ║`);
  console.log(`║  Daemon URL:         ${DAEMON_URL.padEnd(40)}║`);
  console.log('╠══════════════════════════════════════════════════════════════╣');
  console.log('║  Endpoints:                                                  ║');
  console.log('║    GET  /health         - Check bridge status                ║');
  console.log('║    GET  /status         - Check daemon & registration status ║');
  console.log('║    POST /register       - Register single fingerprint        ║');
  console.log('║    POST /authenticate   - Get nullifier for voting           ║');
  console.log('║    POST /verify         - Verify a nullifier                 ║');
  console.log('║  X-Lock (Multi-Finger):                                      ║');
  console.log('║    GET  /enroll-fingers-stream - SSE enrollment (recommended)║');
  console.log('║    POST /enroll-fingers - Enroll 3 fingers (no progress)     ║');
  console.log('║    POST /verify-finger  - Verify with ANY enrolled finger    ║');
  console.log('║  Noir ZK Proofs:                                             ║');
  console.log('║    POST /prove-person   - Generate Groth16 proof for voting  ║');
  console.log('╚══════════════════════════════════════════════════════════════╝');
  console.log('');
});
