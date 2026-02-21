import type { IndexRequest, IndexResponse } from '../contracts';
/**
 * Registry Indexing Agent
 *
 * Handles asset indexing operations: full, incremental, and rebuild modes.
 * Delegates to the underlying registry search/indexing services.
 */
export declare function handleIndex(body: IndexRequest): Promise<IndexResponse>;
//# sourceMappingURL=index-agent.d.ts.map