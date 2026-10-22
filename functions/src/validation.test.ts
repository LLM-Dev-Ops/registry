import { handler } from './index';

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

function createMockReq(path: string, body: unknown, correlationId?: string): any {
  return {
    method: 'POST',
    url: path,
    path,
    headers: {
      host: 'localhost',
      ...(correlationId ? { 'x-correlation-id': correlationId } : {}),
    },
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

async function request(path: string, body: unknown, correlationId?: string): Promise<any> {
  const res = createMockRes();
  await handler(createMockReq(path, body, correlationId), res);
  return { status: res._statusCode, body: JSON.parse(res._body) };
}

async function runTests(): Promise<void> {
  console.log('\n=== Registry Agents Contract Boundary Tests ===\n');

  const invalidCases: Array<[string, unknown, string]> = [
    ['/v1/registry/index', { mode: 'bogus' }, 'rejects an invalid index mode'],
    ['/v1/registry/index', { mode: 'full', asset_ids: [1] }, 'rejects a non-string asset ID'],
    ['/v1/registry/reputation', { agent_id: 'a', operation: 'record' }, 'requires signal for record'],
    [
      '/v1/registry/reputation',
      { agent_id: 'a', operation: 'record', signal: { score: 2, category: 'accuracy' } },
      'rejects an out-of-range reputation score',
    ],
    [
      '/v1/registry/reputation',
      { agent_id: 'a', operation: 'record', signal: { score: 0.5, category: 'bad' } },
      'rejects an invalid reputation category',
    ],
    [
      '/v1/registry/reputation',
      { agent_id: 'a', operation: 'record', signal: { score: 0.5, category: 'accuracy', extra: true } },
      'rejects an unknown nested signal field',
    ],
    ['/v1/registry/bootstrap', { template_id: 1, agent_name: false }, 'rejects invalid bootstrap scalar types'],
    [
      '/v1/registry/bootstrap',
      { template_id: 't', agent_name: 'a', config_overrides: [] },
      'rejects an array used as config_overrides',
    ],
  ];

  for (const [path, body, description] of invalidCases) {
    const response = await request(path, body);
    assert(response.status === 400, description);
    assert(response.body.layers_executed[1].status === 'failed', `${description} marks the agent layer failed`);
  }

  const validRecord = await request('/v1/registry/reputation', {
    agent_id: 'a',
    operation: 'record',
    signal: { score: 0.75, category: 'accuracy', evidence: 'evaluation-run-42' },
  });
  assert(validRecord.status === 200, 'preserves a valid reputation record request');

  const longCorrelationId = 'x'.repeat(129);
  const sanitized = await request('/v1/registry/index', { mode: 'full' }, longCorrelationId);
  assert(sanitized.status === 200, 'accepts a valid request with an invalid correlation header');
  assert(sanitized.body.execution_metadata.trace_id !== longCorrelationId, 'replaces an oversized correlation ID');

  const circular: Record<string, unknown> = {};
  circular.self = circular;
  const internalError = await request('/v1/registry/bootstrap', {
    template_id: 'template',
    agent_name: 'agent',
    config_overrides: circular,
  });
  assert(internalError.status === 500, 'returns 500 for an internal serialization failure');
  assert(internalError.body.error === 'Internal agent error', 'does not disclose internal error details');

  console.log(`\n=== Results: ${passed} passed, ${failed} failed ===\n`);
  process.exit(failed > 0 ? 1 : 0);
}

runTests().catch((error) => {
  console.error('Contract boundary test runner crashed:', error);
  process.exit(1);
});
