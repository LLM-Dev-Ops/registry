import { handler } from './index';
import { IncomingMessage, ServerResponse } from 'node:http';
import { Socket } from 'node:net';

// ============================================================================
// Minimal test harness (no external test framework needed)
// ============================================================================

let passed = 0;
let failed = 0;

function assert(condition: boolean, message: string): void {
  if (condition) {
    passed++;
    console.log(`  PASS: ${message}`);
  } else {
    failed++;
    console.error(`  FAIL: ${message}`);
  }
}

function createMockReq(method: string, path: string, body?: unknown, headers?: Record<string, string>): any {
  return {
    method,
    url: path,
    path,
    headers: { host: 'localhost', ...headers },
    body,
  };
}

function createMockRes(): any {
  const res: any = {
    _statusCode: 0,
    _headers: {} as Record<string, string>,
    _body: '',
    writeHead(code: number, headers?: Record<string, string>) {
      res._statusCode = code;
      if (headers) Object.assign(res._headers, headers);
      return res;
    },
    setHeader(key: string, value: string) {
      res._headers[key] = value;
      return res;
    },
    end(data?: string) {
      res._body = data || '';
    },
  };
  return res;
}

function parseBody(res: any): any {
  return JSON.parse(res._body);
}

// ============================================================================
// Tests
// ============================================================================

async function runTests(): Promise<void> {
  console.log('\n=== Registry Agents Cloud Function Tests ===\n');

  // --- Health endpoint ---
  console.log('Health Endpoint:');
  {
    const req = createMockReq('GET', '/health');
    const res = createMockRes();
    await handler(req, res);
    const body = parseBody(res);

    assert(res._statusCode === 200, 'Health returns 200');
    assert(body.status === 'healthy', 'Health status is healthy');
    assert(JSON.stringify(body.agents) === '["index","reputation","bootstrap"]', 'Lists all 3 agents');
    assert(body.execution_metadata !== undefined, 'Includes execution_metadata');
    assert(body.execution_metadata.service === 'registry-agents', 'Service name is registry-agents');
    assert(body.execution_metadata.trace_id !== undefined, 'Has trace_id');
    assert(body.execution_metadata.execution_id !== undefined, 'Has execution_id');
    assert(body.execution_metadata.timestamp !== undefined, 'Has timestamp');
    assert(Array.isArray(body.layers_executed), 'Includes layers_executed');
    assert(body.layers_executed[0].layer === 'AGENT_ROUTING', 'First layer is AGENT_ROUTING');
  }

  // --- CORS preflight ---
  console.log('\nCORS Preflight:');
  {
    const req = createMockReq('OPTIONS', '/v1/registry/index');
    const res = createMockRes();
    await handler(req, res);

    assert(res._statusCode === 204, 'OPTIONS returns 204');
    assert(res._headers['Access-Control-Allow-Origin'] === '*', 'CORS origin header set');
    assert(res._headers['Access-Control-Allow-Methods'].includes('POST'), 'CORS methods include POST');
  }

  // --- CORS headers on all responses ---
  console.log('\nCORS on Responses:');
  {
    const req = createMockReq('GET', '/health');
    const res = createMockRes();
    await handler(req, res);

    assert(res._headers['Access-Control-Allow-Origin'] === '*', 'Health response has CORS header');
  }

  // --- X-Correlation-Id propagation ---
  console.log('\nCorrelation ID Propagation:');
  {
    const req = createMockReq('GET', '/health', undefined, { 'x-correlation-id': 'test-trace-123' });
    const res = createMockRes();
    await handler(req, res);
    const body = parseBody(res);

    assert(body.execution_metadata.trace_id === 'test-trace-123', 'trace_id uses x-correlation-id');
  }

  // --- Index agent ---
  console.log('\nRegistry Indexing Agent (/v1/registry/index):');
  {
    const req = createMockReq('POST', '/v1/registry/index', {
      mode: 'full',
      asset_ids: ['asset-1', 'asset-2'],
    });
    const res = createMockRes();
    await handler(req, res);
    const body = parseBody(res);

    assert(res._statusCode === 200, 'Index returns 200');
    assert(body.data.indexed_count === 2, 'Indexed count matches input');
    assert(body.data.mode === 'full', 'Mode matches input');
    assert(body.execution_metadata.service === 'registry-agents', 'Has execution_metadata');
    assert(body.layers_executed.length === 2, 'Has 2 layers');
    assert(body.layers_executed[0].layer === 'AGENT_ROUTING', 'First layer is AGENT_ROUTING');
    assert(body.layers_executed[1].layer === 'REGISTRY_INDEX', 'Second layer is REGISTRY_INDEX');
    assert(typeof body.layers_executed[1].duration_ms === 'number', 'Has duration_ms');
  }

  // --- Reputation agent ---
  console.log('\nAgent Reputation Agent (/v1/registry/reputation):');
  {
    const req = createMockReq('POST', '/v1/registry/reputation', {
      agent_id: 'agent-42',
      operation: 'query',
    });
    const res = createMockRes();
    await handler(req, res);
    const body = parseBody(res);

    assert(res._statusCode === 200, 'Reputation returns 200');
    assert(body.data.agent_id === 'agent-42', 'Agent ID matches input');
    assert(typeof body.data.overall_score === 'number', 'Has overall_score');
    assert(body.layers_executed[1].layer === 'REGISTRY_REPUTATION', 'Layer is REGISTRY_REPUTATION');
  }

  // --- Bootstrap agent ---
  console.log('\nTemplate Bootstrap Agent (/v1/registry/bootstrap):');
  {
    const req = createMockReq('POST', '/v1/registry/bootstrap', {
      template_id: 'tmpl-1',
      agent_name: 'my-agent',
    });
    const res = createMockRes();
    await handler(req, res);
    const body = parseBody(res);

    assert(res._statusCode === 200, 'Bootstrap returns 200');
    assert(body.data.agent_name === 'my-agent', 'Agent name matches input');
    assert(body.data.template_id === 'tmpl-1', 'Template ID matches input');
    assert(body.data.status === 'created', 'Status is created');
    assert(body.data.endpoints.health !== undefined, 'Has health endpoint');
    assert(body.data.endpoints.invoke !== undefined, 'Has invoke endpoint');
    assert(body.layers_executed[1].layer === 'REGISTRY_BOOTSTRAP', 'Layer is REGISTRY_BOOTSTRAP');
  }

  // --- Validation: missing required field ---
  console.log('\nValidation - Missing Required Field:');
  {
    const req = createMockReq('POST', '/v1/registry/index', { asset_ids: ['a'] });
    const res = createMockRes();
    await handler(req, res);
    const body = parseBody(res);

    assert(res._statusCode === 400, 'Returns 400 for missing field');
    assert(body.error.includes('mode'), 'Error mentions missing field');
    assert(body.execution_metadata !== undefined, 'Error response has execution_metadata');
  }

  // --- Validation: unknown field ---
  console.log('\nValidation - Unknown Field:');
  {
    const req = createMockReq('POST', '/v1/registry/index', { mode: 'full', bogus: true });
    const res = createMockRes();
    await handler(req, res);
    const body = parseBody(res);

    assert(res._statusCode === 400, 'Returns 400 for unknown field');
    assert(body.error.includes('bogus'), 'Error mentions unknown field');
  }

  // --- Method not allowed ---
  console.log('\nMethod Not Allowed:');
  {
    const req = createMockReq('GET', '/v1/registry/index');
    const res = createMockRes();
    await handler(req, res);
    const body = parseBody(res);

    assert(res._statusCode === 405, 'Returns 405 for GET on agent route');
    assert(body.execution_metadata !== undefined, '405 response has execution_metadata');
  }

  // --- 404 for unknown route ---
  console.log('\n404 Unknown Route:');
  {
    const req = createMockReq('GET', '/v1/registry/nonexistent');
    const res = createMockRes();
    await handler(req, res);
    const body = parseBody(res);

    assert(res._statusCode === 404, 'Returns 404 for unknown route');
    assert(body.available_routes !== undefined, 'Includes available_routes');
    assert(body.execution_metadata !== undefined, '404 response has execution_metadata');
  }

  // --- Contracts endpoint ---
  console.log('\nContracts Endpoint:');
  {
    const req = createMockReq('GET', '/contracts');
    const res = createMockRes();
    await handler(req, res);
    const body = parseBody(res);

    assert(res._statusCode === 200, 'Contracts returns 200');
    assert(body.contracts.index !== undefined, 'Has index contract');
    assert(body.contracts.reputation !== undefined, 'Has reputation contract');
    assert(body.contracts.bootstrap !== undefined, 'Has bootstrap contract');
    assert(body.contracts.index.request !== undefined, 'Index has request schema');
    assert(body.contracts.index.response !== undefined, 'Index has response schema');
  }

  // --- Summary ---
  console.log(`\n=== Results: ${passed} passed, ${failed} failed ===\n`);
  process.exit(failed > 0 ? 1 : 0);
}

runTests().catch((err) => {
  console.error('Test runner crashed:', err);
  process.exit(1);
});
