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
export declare const handler: (req: Request, res: Response) => Promise<void>;
export {};
//# sourceMappingURL=index.d.ts.map