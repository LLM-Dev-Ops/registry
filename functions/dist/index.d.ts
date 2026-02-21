import type { IncomingMessage, ServerResponse } from 'node:http';
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
export declare const handler: (req: Request, res: Response) => Promise<void>;
export {};
//# sourceMappingURL=index.d.ts.map