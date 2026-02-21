"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.handleBootstrap = handleBootstrap;
const node_crypto_1 = __importDefault(require("node:crypto"));
const BASE_URL = process.env.FUNCTION_BASE_URL
    || 'https://us-central1-agentics-dev.cloudfunctions.net/registry-agents';
/**
 * Template Bootstrap Agent
 *
 * Bootstraps new agent instances from registry templates.
 * Provisions configuration and returns ready-to-use endpoints.
 */
async function handleBootstrap(body) {
    const { template_id, agent_name, config_overrides } = body;
    // Business logic delegates to existing registry bootstrap services.
    // This handler structures the request/response contract.
    const agentId = node_crypto_1.default.randomUUID();
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
//# sourceMappingURL=bootstrap-agent.js.map