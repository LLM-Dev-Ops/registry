"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.handler = void 0;
const node_crypto_1 = __importDefault(require("node:crypto"));
const index_agent_1 = require("./agents/index-agent");
const reputation_agent_1 = require("./agents/reputation-agent");
const bootstrap_agent_1 = require("./agents/bootstrap-agent");
const contracts_1 = require("./contracts");
// ============================================================================
// Constants
// ============================================================================
const SERVICE_NAME = 'registry-agents';
const HEALTH_AGENTS = ['index', 'reputation', 'bootstrap'];
const CORS_HEADERS = {
    'Access-Control-Allow-Origin': '*',
    'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
    'Access-Control-Allow-Headers': 'Content-Type, Authorization, X-Correlation-Id',
    'Access-Control-Max-Age': '3600',
};
// ============================================================================
// Helpers
// ============================================================================
function buildExecutionMetadata(req) {
    const correlationId = req.headers['x-correlation-id'];
    const traceId = (Array.isArray(correlationId) ? correlationId[0] : correlationId) || node_crypto_1.default.randomUUID();
    return {
        trace_id: traceId,
        timestamp: new Date().toISOString(),
        service: SERVICE_NAME,
        execution_id: node_crypto_1.default.randomUUID(),
    };
}
function applyCors(res) {
    for (const [key, value] of Object.entries(CORS_HEADERS)) {
        res.setHeader(key, value);
    }
}
function sendJson(res, statusCode, body) {
    res.writeHead(statusCode, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(body));
}
function getRequestPath(req) {
    // Cloud Functions may provide req.path, fall back to parsing req.url
    return req.path || new URL(req.url || '/', `http://${req.headers.host || 'localhost'}`).pathname;
}
function validateRequestBody(body, schemaName) {
    if (!body || typeof body !== 'object') {
        return 'Request body must be a JSON object';
    }
    const schema = contracts_1.CONTRACT_SCHEMAS[schemaName].request;
    const required = schema.required;
    const obj = body;
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
async function routeAgent(agentName, body) {
    const start = performance.now();
    let data;
    switch (agentName) {
        case 'index':
            data = await (0, index_agent_1.handleIndex)(body);
            break;
        case 'reputation':
            data = await (0, reputation_agent_1.handleReputation)(body);
            break;
        case 'bootstrap':
            data = await (0, bootstrap_agent_1.handleBootstrap)(body);
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
function handleHealth(req, res) {
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
function handleContracts(req, res) {
    const metadata = buildExecutionMetadata(req);
    sendJson(res, 200, {
        contracts: contracts_1.CONTRACT_SCHEMAS,
        execution_metadata: metadata,
        layers_executed: [
            { layer: 'AGENT_ROUTING', status: 'completed' },
        ],
    });
}
async function handleAgentRequest(agentName, req, res) {
    const metadata = buildExecutionMetadata(req);
    const layers = [{ layer: 'AGENT_ROUTING', status: 'completed' }];
    if (req.method !== 'POST') {
        sendJson(res, 405, {
            error: 'Method not allowed. Use POST.',
            execution_metadata: metadata,
            layers_executed: layers,
        });
        return;
    }
    // Validate request body against contract schema
    const validationError = validateRequestBody(req.body, agentName);
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
    }
    catch (err) {
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
const AGENT_ROUTES = {
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
const handler = async (req, res) => {
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
exports.handler = handler;
//# sourceMappingURL=index.js.map