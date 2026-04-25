export interface StatusResponse {
  provider: string | null;
  model: string;
  temperature: number;
  uptime_seconds: number;
  gateway_port: number;
  locale: string;
  memory_backend: string;
  paired: boolean;
  channels: Record<string, boolean>;
  health: HealthSnapshot;
}

export interface HealthSnapshot {
  pid: number;
  updated_at: string;
  uptime_seconds: number;
  components: Record<string, ComponentHealth>;
}

export interface ComponentHealth {
  status: string;
  updated_at: string;
  last_ok: string | null;
  last_error: string | null;
  restart_count: number;
}

export interface ToolSpec {
  name: string;
  description: string;
  parameters: any;
}

export interface CronJob {
  id: string;
  name: string | null;
  expression: string;
  command: string;
  prompt: string | null;
  job_type: string;
  schedule: unknown;
  enabled: boolean;
  delivery: unknown;
  delete_after_run: boolean;
  created_at: string;
  next_run: string;
  last_run: string | null;
  last_status: string | null;
  last_output: string | null;
}

export interface CronRun {
  id: number;
  job_id: string;
  started_at: string;
  finished_at: string;
  status: string;
  output: string | null;
  duration_ms: number | null;
}

export interface Integration {
  name: string;
  description: string;
  category: string;
  status: 'Available' | 'Active' | 'ComingSoon';
}

export interface DiagResult {
  severity: 'ok' | 'warn' | 'error';
  category: string;
  message: string;
}

export interface MemoryEntry {
  id: string;
  key: string;
  content: string;
  category: string;
  timestamp: string;
  session_id: string | null;
  score: number | null;
}

export interface CostSummary {
  session_cost_usd: number;
  daily_cost_usd: number;
  monthly_cost_usd: number;
  total_tokens: number;
  request_count: number;
  by_model: Record<string, ModelStats>;
}

export interface ModelStats {
  model: string;
  cost_usd: number;
  total_tokens: number;
  request_count: number;
}

export interface CliTool {
  name: string;
  path: string;
  version: string | null;
  category: string;
}

export interface Session {
  session_id: string;
  created_at: string;
  last_activity: string;
  message_count: number;
  name?: string;
}

export interface ChannelDetail {
  name: string;
  type: string;
  enabled: boolean;
  status: 'active' | 'inactive' | 'error';
  message_count: number;
  last_message_at: string | null;
  health: 'healthy' | 'degraded' | 'down';
}

export interface SSEEvent {
  type: string;
  timestamp?: string;
  [key: string]: any;
}

export interface WsMessage {
  type:
    | 'message'
    | 'chunk'
    | 'chunk_reset'
    | 'thinking'
    | 'tool_call'
    | 'tool_result'
    | 'done'
    | 'error'
    | 'session_start'
    | 'connected'
    | 'cron_result';
  content?: string;
  full_response?: string;
  name?: string;
  args?: any;
  output?: string;
  message?: string;
  code?: string;
  session_id?: string;
  resumed?: boolean;
  message_count?: number;
  timestamp?: string;
  job_id?: string;
  success?: boolean;
  /** Display name of the model that produced this response (set by Model Mesh) */
  model_name?: string;
}

/** Row from GET /api/sessions/{id}/messages */
export interface SessionMessageRow {
  role: string;
  content: string;
}

export interface SessionMessagesResponse {
  session_id: string;
  messages: SessionMessageRow[];
  session_persistence: boolean;
}

// ---------------------------------------------------------------------------
// Hardware telemetry
// ---------------------------------------------------------------------------

export interface HardwareTelemetry {
  cpu_percent: number;
  ram_used_gb: number;
  ram_total_gb: number;
  ram_percent: number;
  gpu_percent: number;
  vram_used_gb: number;
  vram_total_gb: number;
  model_loaded: boolean;
  loaded_model?: string;
}

// ---------------------------------------------------------------------------
// Mexius embedded tool status
// ---------------------------------------------------------------------------

export interface MexiusTool {
  name: string;
  tool_id: string;
  description: string;
  available: boolean;
  locked: boolean;
  icon: string;
}

// ---------------------------------------------------------------------------
// Ollama VRAM / process status  (/api/ollama/ps proxy)
// ---------------------------------------------------------------------------

export interface OllamaRunningModel {
  name: string;
  model: string;
  size: number;
  digest: string;
  details: {
    parent_model?: string;
    format?: string;
    family?: string;
    parameter_size?: string;
    quantization_level?: string;
  };
  expires_at: string;
  size_vram: number;
}

export interface OllamaVramResponse {
  models: OllamaRunningModel[];
}

// ---------------------------------------------------------------------------
// Nexus Supervisor prompt response
// ---------------------------------------------------------------------------

export interface NexusSupervisorPromptResponse {
  prompt: string;
}

// ---------------------------------------------------------------------------
// Nexus / Chain-of-Thought
// ---------------------------------------------------------------------------

export interface NexusMessage {
  type: 'nexus_connected' | 'heartbeat' | 'thinking' | 'reasoning_start' | 'reasoning_end';
  message?: string;
  content?: string;
  stream?: string;
  tick?: number;
  timestamp?: string;
}

export interface OllamaModelsResponse {
  provider: 'ollama';
  reachable: boolean;
  models: string[];
  error?: string;
}

export interface OllamaPullResponse {
  status: 'started';
  model: string;
  message: string;
}

// ─── Sovereignty / Modes ───────────────────────────────────────────────────

export type SovereigntyStateValue = 'idle' | 'active' | 'dreaming' | 'nexus';

export interface StateStatusResponse {
  state: SovereigntyStateValue;
  db_read_only: boolean;
  user_input_enabled: boolean;
}

// ─── Nexus Agent Events ────────────────────────────────────────────────────

export interface NexusAgentEvent {
  type: 'agent_delegation' | 'agent_result' | 'heartbeat' | 'nexus_connected';
  from_agent?: string;
  to_agent?: string;
  task?: string;
  result?: string | null;
  timestamp?: string;
  tick?: number;
  sovereignty_state?: SovereigntyStateValue;
  stream?: string;
  message?: string;
}

// ─── Model Registry ────────────────────────────────────────────────────────

export type ModelSource = 'ollama' | 'openai' | 'anthropic' | 'custom';

export interface RegisteredModel {
  id: string;
  custom_name: string;
  display_name?: string;
  model_id: string;
  api_endpoint: string;
  api_key?: string;
  source: ModelSource;
  is_active: boolean;
  created_at: string;
}

export interface RegisterModelRequest {
  custom_name: string;
  display_name?: string;
  model_id: string;
  api_endpoint: string;
  api_key?: string;
  source: ModelSource;
}
