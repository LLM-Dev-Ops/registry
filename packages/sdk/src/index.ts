import axios, { AxiosInstance, AxiosRequestConfig } from 'axios';

// ============================================================================
// Agentics Execution System Types
// ============================================================================

/**
 * Execution context for agentics system tracing.
 * Must be provided for all /v1/* API calls.
 */
export interface ExecutionContext {
  /** Unique identifier for the execution (assigned by the Core) */
  executionId: string;
  /** Parent span ID from the calling entity in the agentics DAG */
  parentSpanId: string;
}

/** An artifact attached to an agent-level span */
export interface SpanArtifact {
  name: string;
  content_type?: string;
  data: any;
}

/** A single execution span (repo or agent level) */
export interface ExecutionSpan {
  span_id: string;
  parent_span_id: string;
  span_type: 'repo' | 'agent';
  name: string;
  started_at: string;
  ended_at?: string;
  status: 'ok' | 'failed';
  artifacts: SpanArtifact[];
  attributes: Record<string, any>;
}

/** The execution result returned with every /v1/* response */
export interface ExecutionResult {
  execution_id: string;
  spans: ExecutionSpan[];
}

/** Response envelope wrapping data alongside execution spans */
export interface ExecutionEnvelope<T> {
  data: T;
  execution: ExecutionResult;
  meta?: Record<string, any>;
}

/** Paginated response with execution spans */
export interface PaginatedExecutionEnvelope<T> {
  items: T[];
  pagination: {
    total: number;
    offset: number;
    limit: number;
    has_more: boolean;
  };
  execution: ExecutionResult;
}

/** Error response that may include execution spans */
export interface ErrorResponse {
  status: number;
  error: string;
  code?: string;
  timestamp: string;
  execution?: ExecutionResult;
}

// ============================================================================
// Configuration & Domain Types
// ============================================================================

/**
 * Configuration options for the LLM Registry client
 */
export interface LLMRegistryConfig {
  /** Base URL of the LLM Registry API */
  baseURL: string;
  /** API token for authentication (optional) */
  apiToken?: string;
  /** Request timeout in milliseconds (default: 30000) */
  timeout?: number;
  /** Default execution context applied to all /v1/* requests */
  executionContext?: ExecutionContext;
  /** Additional axios configuration */
  axiosConfig?: AxiosRequestConfig;
}

/**
 * Model information
 */
export interface Model {
  id: string;
  name: string;
  version: string;
  description?: string;
  provider?: string;
  created_at: string;
  updated_at: string;
  tags?: string[];
  metadata?: Record<string, any>;
}

/**
 * Asset information
 */
export interface Asset {
  id: string;
  model_id: string;
  name: string;
  version: string;
  content_type: string;
  size: number;
  checksum: string;
  storage_path: string;
  created_at: string;
  metadata?: Record<string, any>;
}

/**
 * Model creation request
 */
export interface CreateModelRequest {
  name: string;
  version: string;
  description?: string;
  provider?: string;
  tags?: string[];
  metadata?: Record<string, any>;
}

/**
 * Asset upload request
 */
export interface UploadAssetRequest {
  model_id: string;
  name: string;
  version: string;
  content_type: string;
  file: Buffer | Blob;
  metadata?: Record<string, any>;
}

/**
 * Search filters
 */
export interface SearchFilters {
  query?: string;
  provider?: string;
  tags?: string[];
  limit?: number;
  offset?: number;
}

// ============================================================================
// SDK Client
// ============================================================================

/**
 * LLM Registry SDK Client
 *
 * A TypeScript client for interacting with the LLM Registry API.
 * Supports the Agentics execution system by injecting execution context
 * headers (X-Execution-Id, X-Parent-Span-Id) into all /v1/* requests.
 *
 * @example
 * ```typescript
 * const client = new LLMRegistryClient({
 *   baseURL: 'http://localhost:8080',
 *   apiToken: 'your-api-token',
 *   executionContext: {
 *     executionId: 'exec-001',
 *     parentSpanId: '01HQWX...',
 *   },
 * });
 *
 * // List models (returns ExecutionEnvelope with spans)
 * const models = await client.listModels();
 * ```
 */
export class LLMRegistryClient {
  private client: AxiosInstance;
  private executionContext?: ExecutionContext;

  constructor(config: LLMRegistryConfig) {
    const { baseURL, apiToken, timeout = 30000, axiosConfig = {} } = config;

    this.executionContext = config.executionContext;

    this.client = axios.create({
      baseURL,
      timeout,
      headers: {
        'Content-Type': 'application/json',
        ...(apiToken && { Authorization: `Bearer ${apiToken}` }),
        ...axiosConfig.headers,
      },
      ...axiosConfig,
    });

    // Inject execution context headers into every request
    this.client.interceptors.request.use((reqConfig) => {
      const ctx = this.executionContext;
      if (ctx) {
        reqConfig.headers = reqConfig.headers || {};
        reqConfig.headers['X-Execution-Id'] = ctx.executionId;
        reqConfig.headers['X-Parent-Span-Id'] = ctx.parentSpanId;
      }
      return reqConfig;
    });
  }

  /**
   * Update the execution context for subsequent requests.
   */
  setExecutionContext(ctx: ExecutionContext): void {
    this.executionContext = ctx;
  }

  // Models API

  /**
   * List all models
   */
  async listModels(filters?: SearchFilters): Promise<Model[]> {
    const response = await this.client.get<Model[]>('/api/v1/models', {
      params: filters,
    });
    return response.data;
  }

  /**
   * Get a specific model by ID
   */
  async getModel(modelId: string): Promise<Model> {
    const response = await this.client.get<Model>(`/api/v1/models/${modelId}`);
    return response.data;
  }

  /**
   * Create a new model
   */
  async createModel(request: CreateModelRequest): Promise<Model> {
    const response = await this.client.post<Model>('/api/v1/models', request);
    return response.data;
  }

  /**
   * Update a model
   */
  async updateModel(modelId: string, updates: Partial<CreateModelRequest>): Promise<Model> {
    const response = await this.client.patch<Model>(`/api/v1/models/${modelId}`, updates);
    return response.data;
  }

  /**
   * Delete a model
   */
  async deleteModel(modelId: string): Promise<void> {
    await this.client.delete(`/api/v1/models/${modelId}`);
  }

  /**
   * Search models
   */
  async searchModels(filters: SearchFilters): Promise<Model[]> {
    const response = await this.client.get<Model[]>('/api/v1/models/search', {
      params: filters,
    });
    return response.data;
  }

  // Assets API

  /**
   * List assets for a model
   */
  async listAssets(modelId: string): Promise<Asset[]> {
    const response = await this.client.get<Asset[]>(`/api/v1/models/${modelId}/assets`);
    return response.data;
  }

  /**
   * Get a specific asset
   */
  async getAsset(modelId: string, assetId: string): Promise<Asset> {
    const response = await this.client.get<Asset>(`/api/v1/models/${modelId}/assets/${assetId}`);
    return response.data;
  }

  /**
   * Upload an asset
   */
  async uploadAsset(request: UploadAssetRequest): Promise<Asset> {
    const formData = new FormData();
    formData.append('name', request.name);
    formData.append('version', request.version);
    formData.append('content_type', request.content_type);
    formData.append('file', request.file as any);

    if (request.metadata) {
      formData.append('metadata', JSON.stringify(request.metadata));
    }

    const response = await this.client.post<Asset>(
      `/api/v1/models/${request.model_id}/assets`,
      formData,
      {
        headers: {
          'Content-Type': 'multipart/form-data',
        },
      }
    );
    return response.data;
  }

  /**
   * Download an asset
   */
  async downloadAsset(modelId: string, assetId: string): Promise<ArrayBuffer> {
    const response = await this.client.get<ArrayBuffer>(
      `/api/v1/models/${modelId}/assets/${assetId}/download`,
      {
        responseType: 'arraybuffer',
      }
    );
    return response.data;
  }

  /**
   * Delete an asset
   */
  async deleteAsset(modelId: string, assetId: string): Promise<void> {
    await this.client.delete(`/api/v1/models/${modelId}/assets/${assetId}`);
  }

  // Health & Status

  /**
   * Check API health status
   */
  async health(): Promise<{ status: string; version?: string }> {
    const response = await this.client.get('/health');
    return response.data;
  }

  /**
   * Get API version
   */
  async version(): Promise<{ version: string }> {
    const response = await this.client.get('/version');
    return response.data;
  }
}

export default LLMRegistryClient;
