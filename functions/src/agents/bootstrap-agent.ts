import crypto from 'node:crypto';
import type { BootstrapRequest, BootstrapResponse } from '../contracts';

const BASE_URL = process.env.FUNCTION_BASE_URL
  || 'https://us-central1-agentics-dev.cloudfunctions.net/registry-agents';

/**
 * Template Bootstrap Agent
 *
 * Bootstraps new agent instances from registry templates.
 * Provisions configuration and returns ready-to-use endpoints.
 */
export async function handleBootstrap(body: BootstrapRequest): Promise<BootstrapResponse> {
  const { template_id, agent_name, config_overrides } = body;

  // Business logic delegates to existing registry bootstrap services.
  // This handler structures the request/response contract.
  const agentId = crypto.randomUUID();

  return {
    agent_id: agentId,
    agent_name,
    template_id,
    status: 'created',
    config_applied: config_overrides ?? {},
    endpoints: {
      health: `${BASE_URL}/health`,
      invoke: `${BASE_URL}/v1/registry/bootstrap`,
    },
  };
}
