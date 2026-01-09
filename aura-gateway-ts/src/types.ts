import { z } from 'zod';

// === Request Types ===

export const RecordSummarySchema = z.object({
  seq: z.number(),
  tx_kind: z.string(),
  action_kinds: z.array(z.string()),
  payload_summary: z.string().optional(),
});

export const TransactionSchema = z.object({
  tx_id: z.string(),
  agent_id: z.string(),
  ts_ms: z.number(),
  kind: z.string(),
  payload: z.string(), // base64
});

export const ProposeLimitsSchema = z.object({
  max_proposals: z.number().default(8),
});

export const ProposeRequestSchema = z.object({
  agent_id: z.string(),
  tx: TransactionSchema,
  record_window: z.array(RecordSummarySchema).default([]),
  limits: ProposeLimitsSchema.default({ max_proposals: 8 }),
});

export type RecordSummary = z.infer<typeof RecordSummarySchema>;
export type Transaction = z.infer<typeof TransactionSchema>;
export type ProposeLimits = z.infer<typeof ProposeLimitsSchema>;
export type ProposeRequest = z.infer<typeof ProposeRequestSchema>;

// === Response Types ===

export const ActionKind = z.enum(['reason', 'memorize', 'decide', 'delegate']);
export type ActionKind = z.infer<typeof ActionKind>;

export const ProposalSchema = z.object({
  action_kind: ActionKind,
  payload: z.string(), // base64 encoded
  rationale: z.string().optional(),
});

export const TraceSchema = z.object({
  model: z.string().optional(),
  latency_ms: z.number().optional(),
  metadata: z.record(z.string()).optional(),
});

export const ProposalSetSchema = z.object({
  proposals: z.array(ProposalSchema),
  trace: TraceSchema.optional(),
});

export type Proposal = z.infer<typeof ProposalSchema>;
export type Trace = z.infer<typeof TraceSchema>;
export type ProposalSet = z.infer<typeof ProposalSetSchema>;

// === Tool Call Types ===

export const ToolCallSchema = z.object({
  tool: z.string(),
  args: z.record(z.any()),
});

export type ToolCall = z.infer<typeof ToolCallSchema>;

// === Config ===

export interface GatewayConfig {
  port: number;
  anthropicApiKey: string;
  model: string;
  maxTokens: number;
}
