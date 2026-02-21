import crypto from 'node:crypto';
import type { IncomingMessage, ServerResponse } from 'node:http';
import { handleIndex } from './agents/index-agent';
import { handleReputation } from './agents/reputation-agent';
import { handleBootstrap } from './agents/bootstrap-agent';
import { CONTRACT_SCHEMAS } from './contracts';

// ============================================================================
// Types
// ============================================================================

interface Request extends IncomingMessage {
  body?: unknown;
  path?: string;
  method?: string;
}

interface Response extends ServerResponse {
  status?(code: number): Response;
  json?(data: unknown): void;
  send?(data: unknown): void;
  set?(header: string, value: string): void;
}

interface ExecutionMetadata {
  trace_id: string;
  timestamp: string;
  service: string;
  execution_id: string;
}

interface LayerEntry {
  layer: string;
  status: 'completed' | 'failed';
  duration_ms?: number;
}

interface AgentResult {
  data: unknown;
  agent_layer: string;
  duration_ms: number;
}

// ============================================================================
// Constants
// ============================================================================

const SERVICE_NAME = 'registry-agents';
const HEALTH_AGENTS = ['index', 'reputation', 'bootstrap'] as const;

const CORS_HEADERS: Record<string, string> = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
  'Access-Control-Allow-Headers': 'Content-Type, Authorization, X-Correlation-Id',
  'Access-Control-Max-Age': '3600',
};

// ============================================================================
// Helpers
// ============================================================================

function buildExecutionMetadata(req: Request): ExecutionMetadata {
  const correlationId = req.headers['x-correlation-id'];
  const traceId = (Array.isArray(correlationId) ? correlationId[0] : correlationId) || crypto.randomUUID();

  return {
    trace_id: traceId,
    timestamp: new Date().toISOString(),
    service: SERVICE_NAME,
    execution_id: crypto.randomUUID(),
  };
}

function applyCors(res: Response): void {
  for (const [key, value] of Object.entries(CORS_HEADERS)) {
    res.setHeader(key, value);
  }
}

function sendJson(res: Response, statusCode: number, body: unknown): void {
  res.writeHead(statusCode, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify(body));
}

function getRequestPath(req: Request): string {
  // Cloud Functions may provide req.path, fall back to parsing req.url
  return req.path || new URL(req.url || '/', `http://${req.headers.host || 'localhost'}`).pathname;
}

function validateRequestBody(body: unknown, schemaName: keyof typeof CONTRACT_SCHEMAS): string | null {
  if (!body || typeof body !== 'object') {
    return 'Request body must be a JSON object';
  }

  const schema = CONTRACT_SCHEMAS[schemaName].request;
  const required = schema.required as readonly string[];
  const obj = body as Record<string, unknown>;

  for (const field of required) {
    if (!(field in obj)) {
      return `Missing required field: ${field}`;
    }
  }

  const allowedProps = Object.keys(schema.properties);
  for (const key of Object.keys(obj)) {
    if (!allowedProps.includes(key)) {
      return `Unknown field: ${key}`;
    }
  }

  return null;
}

// ============================================================================
// Route Handlers
// ============================================================================

async function routeAgent(agentName: string, body: unknown): Promise<AgentResult> {
  const start = performance.now();
  let data: unknown;

  switch (agentName) {
    case 'index':
      data = await handleIndex(body as any);
      break;
    case 'reputation':
      data = await handleReputation(body as any);
      break;
    case 'bootstrap':
      data = await handleBootstrap(body as any);
      break;
    default:
      throw new Error(`Unknown agent: ${agentName}`);
  }

  return {
    data,
    agent_layer: `REGISTRY_${agentName.toUpperCase()}`,
    duration_ms: Math.round(performance.now() - start),
  };
}

function handleHealth(req: Request, res: Response): void {
  const metadata = buildExecutionMetadata(req);

  sendJson(res, 200, {
    status: 'healthy',
    agents: HEALTH_AGENTS,
    execution_metadata: metadata,
    layers_executed: [
      { layer: 'AGENT_ROUTING', status: 'completed' },
    ],
  });
}

function handleContracts(req: Request, res: Response): void {
  const metadata = buildExecutionMetadata(req);

  sendJson(res, 200, {
    contracts: CONTRACT_SCHEMAS,
    execution_metadata: metadata,
    layers_executed: [
      { layer: 'AGENT_ROUTING', status: 'completed' },
    ],
  });
}

async function handleAgentRequest(
  agentName: string,
  req: Request,
  res: Response,
): Promise<void> {
  const metadata = buildExecutionMetadata(req);
  const layers: LayerEntry[] = [{ layer: 'AGENT_ROUTING', status: 'completed' }];

  if (req.method !== 'POST') {
    sendJson(res, 405, {
      error: 'Method not allowed. Use POST.',
      execution_metadata: metadata,
      layers_executed: layers,
    });
    return;
  }

  // Validate request body against contract schema
  const validationError = validateRequestBody(
    req.body,
    agentName as keyof typeof CONTRACT_SCHEMAS,
  );
  if (validationError) {
    layers.push({ layer: `REGISTRY_${agentName.toUpperCase()}`, status: 'failed', duration_ms: 0 });
    sendJson(res, 400, {
      error: validationError,
      execution_metadata: metadata,
      layers_executed: layers,
    });
    return;
  }

  try {
    const result = await routeAgent(agentName, req.body);
    layers.push({
      layer: result.agent_layer,
      status: 'completed',
      duration_ms: result.duration_ms,
    });

    sendJson(res, 200, {
      data: result.data,
      execution_metadata: metadata,
      layers_executed: layers,
    });
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : 'Internal agent error';
    layers.push({
      layer: `REGISTRY_${agentName.toUpperCase()}`,
      status: 'failed',
      duration_ms: 0,
    });

    sendJson(res, 500, {
      error: message,
      execution_metadata: metadata,
      layers_executed: layers,
    });
  }
}

// ============================================================================
// Entry Point
// ============================================================================

const AGENT_ROUTES: Record<string, string> = {
  '/v1/registry/index': 'index',
  '/v1/registry/reputation': 'reputation',
  '/v1/registry/bootstrap': 'bootstrap',
};

/**
 * Cloud Function HTTP handler for registry-agents.
 *
 * Routes:
 *   GET  /health                    - Health check (lists all agents)
 *   GET  /contracts                 - Contract schemas for all agents
 *   POST /v1/registry/index         - Registry Indexing Agent
 *   POST /v1/registry/reputation    - Agent Reputation Agent
 *   POST /v1/registry/bootstrap     - Template Bootstrap Agent
 */
export const handler = async (req: Request, res: Response): Promise<void> => {
  applyCors(res);

  // Handle CORS preflight
  if (req.method === 'OPTIONS') {
    res.writeHead(204);
    res.end();
    return;
  }

  const path = getRequestPath(req);

  // Health endpoint
  if (path === '/health' || path === '/') {
    handleHealth(req, res);
    return;
  }

  // Contracts endpoint
  if (path === '/contracts') {
    handleContracts(req, res);
    return;
  }

  // Agent routes
  const agentName = AGENT_ROUTES[path];
  if (agentName) {
    await handleAgentRequest(agentName, req, res);
    return;
  }

  // 404 for unmatched routes
  const metadata = buildExecutionMetadata(req);
  sendJson(res, 404, {
    error: `Route not found: ${path}`,
    available_routes: ['/health', '/contracts', ...Object.keys(AGENT_ROUTES)],
    execution_metadata: metadata,
    layers_executed: [{ layer: 'AGENT_ROUTING', status: 'failed' }],
  });
};
