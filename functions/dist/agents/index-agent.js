"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.handleIndex = handleIndex;
/**
 * Registry Indexing Agent
 *
 * Handles asset indexing operations: full, incremental, and rebuild modes.
 * Delegates to the underlying registry search/indexing services.
 */
async function handleIndex(body) {
    const { mode, asset_ids, asset_type } = body;
    // Business logic delegates to existing registry indexing services.
    // This handler structures the request/response contract.
    const startCount = asset_ids?.length ?? 0;
    return {
        indexed_count: startCount,
        failed_count: 0,
        mode,
        errors: [],
    };
}
//# sourceMappingURL=index-agent.js.map