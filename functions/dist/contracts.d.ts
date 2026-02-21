export interface IndexRequest {
    /** Asset IDs to index, or empty to trigger full re-index */
    asset_ids?: string[];
    /** Index operation mode */
    mode: 'full' | 'incremental' | 'rebuild';
    /** Optional filter by asset type */
    asset_type?: 'MODEL' | 'PIPELINE' | 'TEST_SUITE' | 'POLICY' | 'DATASET';
}
export interface IndexResponse {
    indexed_count: number;
    failed_count: number;
    mode: string;
    errors: Array<{
        asset_id: string;
        error: string;
    }>;
}
export interface ReputationRequest {
    /** Agent identifier to query or update reputation for */
    agent_id: string;
    /** Operation: 'query' to read, 'record' to submit a new signal */
    operation: 'query' | 'record';
    /** Signal to record (required when operation is 'record') */
    signal?: {
        score: number;
        category: 'reliability' | 'accuracy' | 'latency' | 'compliance';
        evidence?: string;
    };
}
export interface ReputationResponse {
    agent_id: string;
    overall_score: number;
    category_scores: Record<string, number>;
    signal_count: number;
    last_updated: string;
}
export interface BootstrapRequest {
    /** Template identifier to bootstrap from */
    template_id: string;
    /** Name for the new agent instance */
    agent_name: string;
    /** Configuration overrides for the bootstrapped agent */
    config_overrides?: Record<string, unknown>;
}
export interface BootstrapResponse {
    agent_id: string;
    agent_name: string;
    template_id: string;
    status: 'created' | 'pending' | 'failed';
    config_applied: Record<string, unknown>;
    endpoints: {
        health: string;
        invoke: string;
    };
}
export declare const CONTRACT_SCHEMAS: {
    readonly index: {
        readonly request: {
            readonly type: "object";
            readonly required: readonly ["mode"];
            readonly properties: {
                readonly asset_ids: {
                    readonly type: "array";
                    readonly items: {
                        readonly type: "string";
                    };
                };
                readonly mode: {
                    readonly type: "string";
                    readonly enum: readonly ["full", "incremental", "rebuild"];
                };
                readonly asset_type: {
                    readonly type: "string";
                    readonly enum: readonly ["MODEL", "PIPELINE", "TEST_SUITE", "POLICY", "DATASET"];
                };
            };
            readonly additionalProperties: false;
        };
        readonly response: {
            readonly type: "object";
            readonly required: readonly ["indexed_count", "failed_count", "mode", "errors"];
            readonly properties: {
                readonly indexed_count: {
                    readonly type: "number";
                };
                readonly failed_count: {
                    readonly type: "number";
                };
                readonly mode: {
                    readonly type: "string";
                };
                readonly errors: {
                    readonly type: "array";
                    readonly items: {
                        readonly type: "object";
                        readonly properties: {
                            readonly asset_id: {
                                readonly type: "string";
                            };
                            readonly error: {
                                readonly type: "string";
                            };
                        };
                    };
                };
            };
        };
    };
    readonly reputation: {
        readonly request: {
            readonly type: "object";
            readonly required: readonly ["agent_id", "operation"];
            readonly properties: {
                readonly agent_id: {
                    readonly type: "string";
                };
                readonly operation: {
                    readonly type: "string";
                    readonly enum: readonly ["query", "record"];
                };
                readonly signal: {
                    readonly type: "object";
                    readonly properties: {
                        readonly score: {
                            readonly type: "number";
                            readonly minimum: 0;
                            readonly maximum: 1;
                        };
                        readonly category: {
                            readonly type: "string";
                            readonly enum: readonly ["reliability", "accuracy", "latency", "compliance"];
                        };
                        readonly evidence: {
                            readonly type: "string";
                        };
                    };
                    readonly required: readonly ["score", "category"];
                };
            };
            readonly additionalProperties: false;
        };
        readonly response: {
            readonly type: "object";
            readonly required: readonly ["agent_id", "overall_score", "category_scores", "signal_count", "last_updated"];
            readonly properties: {
                readonly agent_id: {
                    readonly type: "string";
                };
                readonly overall_score: {
                    readonly type: "number";
                };
                readonly category_scores: {
                    readonly type: "object";
                };
                readonly signal_count: {
                    readonly type: "number";
                };
                readonly last_updated: {
                    readonly type: "string";
                    readonly format: "date-time";
                };
            };
        };
    };
    readonly bootstrap: {
        readonly request: {
            readonly type: "object";
            readonly required: readonly ["template_id", "agent_name"];
            readonly properties: {
                readonly template_id: {
                    readonly type: "string";
                };
                readonly agent_name: {
                    readonly type: "string";
                };
                readonly config_overrides: {
                    readonly type: "object";
                };
            };
            readonly additionalProperties: false;
        };
        readonly response: {
            readonly type: "object";
            readonly required: readonly ["agent_id", "agent_name", "template_id", "status", "config_applied", "endpoints"];
            readonly properties: {
                readonly agent_id: {
                    readonly type: "string";
                };
                readonly agent_name: {
                    readonly type: "string";
                };
                readonly template_id: {
                    readonly type: "string";
                };
                readonly status: {
                    readonly type: "string";
                    readonly enum: readonly ["created", "pending", "failed"];
                };
                readonly config_applied: {
                    readonly type: "object";
                };
                readonly endpoints: {
                    readonly type: "object";
                    readonly properties: {
                        readonly health: {
                            readonly type: "string";
                        };
                        readonly invoke: {
                            readonly type: "string";
                        };
                    };
                };
            };
        };
    };
};
//# sourceMappingURL=contracts.d.ts.map