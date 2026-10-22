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
const SERVICE_NAME = 'registry-agents';
const HEALTH_AGENTS = ['index', 'reputation', 'bootstrap'];
const CORRELATION_ID_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/;
const CORS_HEADERS = {
    'Access-Control-Allow-Origin': '*',
    'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
    'Access-Control-Allow-Headers': 'Content-Type, Authorization, X-Correlation-Id',
    'Access-Control-Max-Age': '3600',
};
function buildExecutionMetadata(req) {
    const header = req.headers['x-correlation-id'];
    const candidate = Array.isArray(header) ? header[0] : header;
    const traceId = candidate && CORRELATION_ID_PATTERN.test(candidate)
        ? candidate
        : node_crypto_1.default.randomUUID();
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
    return req.path || new URL(req.url || '/', `http://${req.headers.host || 'localhost'}`).pathname;
}
function schemaTypeMatches(value, type) {
    switch (type) {
        case 'object':
            return value !== null && typeof value === 'object' && !Array.isArray(value);
        case 'array':
            return Array.isArray(value);
        case 'number':
            return typeof value === 'number' && Number.isFinite(value);
        case 'integer':
            return typeof value === 'number' && Number.isInteger(value);
        case 'string':
        case 'boolean':
            return typeof value === type;
        default:
            return true;
    }
}
function validateSchema(value, schema, path) {
    if (!schemaTypeMatches(value, schema.type)) {
        return `${path} must be of type ${schema.type}`;
    }
    if (schema.enum && !schema.enum.some((candidate) => Object.is(candidate, value))) {
        return `${path} must be one of: ${schema.enum.join(', ')}`;
    }
    if (typeof value === 'number') {
        if (schema.minimum !== undefined && value < schema.minimum) {
            return `${path} must be at least ${schema.minimum}`;
        }
        if (schema.maximum !== undefined && value > schema.maximum) {
            return `${path} must be at most ${schema.maximum}`;
        }
    }
    if (Array.isArray(value) && schema.items) {
        for (let index = 0; index < value.length; index++) {
            const error = validateSchema(value[index], schema.items, `${path}[${index}]`);
            if (error)
                return error;
        }
    }
    if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
        const objectValue = value;
        const properties = schema.properties || {};
        for (const field of schema.required || []) {
            if (!Object.prototype.hasOwnProperty.call(objectValue, field)) {
                return `Missing required field: ${path === 'body' ? field : `${path}.${field}`}`;
            }
        }
        if (schema.additionalProperties === false) {
            for (const key of Object.keys(objectValue)) {
                if (!Object.prototype.hasOwnProperty.call(properties, key)) {
                    return `Unknown field: ${path === 'body' ? key : `${path}.${key}`}`;
                }
            }
        }
        for (const [key, propertySchema] of Object.entries(properties)) {
            if (Object.prototype.hasOwnProperty.call(objectValue, key)) {
                const error = validateSchema(objectValue[key], propertySchema, `${path}.${key}`);
                if (error)
                    return error;
            }
        }
    }
    return null;
}
function validateRequestBody(body, schemaName) {
    const schema = contracts_1.CONTRACT_SCHEMAS[schemaName].request;
    const error = validateSchema(body, schema, 'body');
    if (error)
        return error;
    if (schemaName === 'reputation') {
        const request = body;
        if (request.operation === 'record' && !Object.prototype.hasOwnProperty.call(request, 'signal')) {
            return 'Missing required field: signal';
        }
    }
    return null;
}
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
        layers_executed: [{ layer: 'AGENT_ROUTING', status: 'completed' }],
    });
}
function handleContracts(req, res) {
    const metadata = buildExecutionMetadata(req);
    sendJson(res, 200, {
        contracts: contracts_1.CONTRACT_SCHEMAS,
        execution_metadata: metadata,
        layers_executed: [{ layer: 'AGENT_ROUTING', status: 'completed' }],
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
        console.error('Registry agent request failed', {
            agent: agentName,
            execution_id: metadata.execution_id,
            error_name: err instanceof Error ? err.name : 'UnknownError',
        });
        layers.push({
            layer: `REGISTRY_${agentName.toUpperCase()}`,
            status: 'failed',
            duration_ms: 0,
        });
        sendJson(res, 500, {
            error: 'Internal agent error',
            execution_metadata: metadata,
            layers_executed: layers,
        });
    }
}
const AGENT_ROUTES = {
    '/v1/registry/index': 'index',
    '/v1/registry/reputation': 'reputation',
    '/v1/registry/bootstrap': 'bootstrap',
};
const handler = async (req, res) => {
    applyCors(res);
    if (req.method === 'OPTIONS') {
        res.writeHead(204);
        res.end();
        return;
    }
    const path = getRequestPath(req);
    if (path === '/health' || path === '/') {
        handleHealth(req, res);
        return;
    }
    if (path === '/contracts') {
        handleContracts(req, res);
        return;
    }
    const agentName = AGENT_ROUTES[path];
    if (agentName) {
        await handleAgentRequest(agentName, req, res);
        return;
    }
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