"use strict";
// ============================================================================
// Contract schemas for the 3 Registry Agents
// ============================================================================
Object.defineProperty(exports, "__esModule", { value: true });
exports.CONTRACT_SCHEMAS = void 0;
// JSON Schema definitions for runtime validation
exports.CONTRACT_SCHEMAS = {
    index: {
        request: {
            type: 'object',
            required: ['mode'],
            properties: {
                asset_ids: { type: 'array', items: { type: 'string' } },
                mode: { type: 'string', enum: ['full', 'incremental', 'rebuild'] },
                asset_type: { type: 'string', enum: ['MODEL', 'PIPELINE', 'TEST_SUITE', 'POLICY', 'DATASET'] },
            },
            additionalProperties: false,
        },
        response: {
            type: 'object',
            required: ['indexed_count', 'failed_count', 'mode', 'errors'],
            properties: {
                indexed_count: { type: 'number' },
                failed_count: { type: 'number' },
                mode: { type: 'string' },
                errors: {
                    type: 'array',
                    items: {
                        type: 'object',
                        properties: {
                            asset_id: { type: 'string' },
                            error: { type: 'string' },
                        },
                    },
                },
            },
        },
    },
    reputation: {
        request: {
            type: 'object',
            required: ['agent_id', 'operation'],
            properties: {
                agent_id: { type: 'string' },
                operation: { type: 'string', enum: ['query', 'record'] },
                signal: {
                    type: 'object',
                    properties: {
                        score: { type: 'number', minimum: 0, maximum: 1 },
                        category: { type: 'string', enum: ['reliability', 'accuracy', 'latency', 'compliance'] },
                        evidence: { type: 'string' },
                    },
                    required: ['score', 'category'],
                },
            },
            additionalProperties: false,
        },
        response: {
            type: 'object',
            required: ['agent_id', 'overall_score', 'category_scores', 'signal_count', 'last_updated'],
            properties: {
                agent_id: { type: 'string' },
                overall_score: { type: 'number' },
                category_scores: { type: 'object' },
                signal_count: { type: 'number' },
                last_updated: { type: 'string', format: 'date-time' },
            },
        },
    },
    bootstrap: {
        request: {
            type: 'object',
            required: ['template_id', 'agent_name'],
            properties: {
                template_id: { type: 'string' },
                agent_name: { type: 'string' },
                config_overrides: { type: 'object' },
            },
            additionalProperties: false,
        },
        response: {
            type: 'object',
            required: ['agent_id', 'agent_name', 'template_id', 'status', 'config_applied', 'endpoints'],
            properties: {
                agent_id: { type: 'string' },
                agent_name: { type: 'string' },
                template_id: { type: 'string' },
                status: { type: 'string', enum: ['created', 'pending', 'failed'] },
                config_applied: { type: 'object' },
                endpoints: {
                    type: 'object',
                    properties: {
                        health: { type: 'string' },
                        invoke: { type: 'string' },
                    },
                },
            },
        },
    },
};
//# sourceMappingURL=contracts.js.map