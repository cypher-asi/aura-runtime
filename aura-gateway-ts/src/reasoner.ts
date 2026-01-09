import Anthropic from '@anthropic-ai/sdk';
import type { GatewayConfig, ProposeRequest, ProposalSet, Proposal, ToolCall } from './types';

/**
 * Reasoner that calls Claude to generate proposals.
 * 
 * IMPORTANT: This is a "propose-only" reasoner. It suggests actions
 * but does NOT execute them. The Rust kernel handles authorization
 * and execution.
 */
export class ClaudeReasoner {
  private client: Anthropic;
  private config: GatewayConfig;

  constructor(config: GatewayConfig) {
    this.config = config;
    this.client = new Anthropic({
      apiKey: config.anthropicApiKey,
    });
  }

  /**
   * Generate proposals for a transaction.
   */
  async propose(request: ProposeRequest): Promise<ProposalSet> {
    const startTime = Date.now();

    try {
      const systemPrompt = this.buildSystemPrompt();
      const userMessage = this.buildUserMessage(request);

      const response = await this.client.messages.create({
        model: this.config.model,
        max_tokens: this.config.maxTokens,
        system: systemPrompt,
        messages: [
          { role: 'user', content: userMessage }
        ],
      });

      const proposals = this.parseResponse(response);
      const latencyMs = Date.now() - startTime;

      return {
        proposals,
        trace: {
          model: this.config.model,
          latency_ms: latencyMs,
          metadata: {
            input_tokens: String(response.usage?.input_tokens ?? 0),
            output_tokens: String(response.usage?.output_tokens ?? 0),
          },
        },
      };
    } catch (error) {
      const latencyMs = Date.now() - startTime;
      console.error('Reasoner error:', error);

      return {
        proposals: [],
        trace: {
          model: this.config.model,
          latency_ms: latencyMs,
          metadata: {
            error: error instanceof Error ? error.message : 'Unknown error',
          },
        },
      };
    }
  }

  private buildSystemPrompt(): string {
    return `You are an AI agent reasoning system. Your role is to analyze transactions and propose actions.

IMPORTANT RULES:
1. You can ONLY propose actions, not execute them.
2. The kernel will authorize and execute your proposals.
3. Respond with a JSON array of proposals.

AVAILABLE ACTIONS:
- reason: Think about the problem
- memorize: Store information for future reference
- decide: Make a decision
- delegate: Delegate to a tool (filesystem read, etc.)

AVAILABLE TOOLS (via delegate):
- fs.ls: List directory contents. Args: { "path": "string" }
- fs.read: Read file contents. Args: { "path": "string", "max_bytes": number? }
- fs.stat: Get file metadata. Args: { "path": "string" }

RESPONSE FORMAT:
Respond with ONLY a JSON object like:
{
  "proposals": [
    {
      "action_kind": "delegate",
      "tool_call": { "tool": "fs.read", "args": { "path": "file.txt" } },
      "rationale": "Why this action is needed"
    }
  ]
}

Keep proposals focused and minimal. Do not propose actions you don't need.`;
  }

  private buildUserMessage(request: ProposeRequest): string {
    // Decode payload from base64
    let payloadText = '';
    try {
      payloadText = Buffer.from(request.tx.payload, 'base64').toString('utf-8');
    } catch {
      payloadText = request.tx.payload;
    }

    let message = `Transaction to process:
- Kind: ${request.tx.kind}
- Payload: ${payloadText}
`;

    if (request.record_window.length > 0) {
      message += `\nRecent history (last ${request.record_window.length} entries):\n`;
      for (const entry of request.record_window.slice(-5)) {
        message += `- seq ${entry.seq}: ${entry.tx_kind}, actions: ${entry.action_kinds.join(', ')}\n`;
      }
    }

    message += `\nPropose up to ${request.limits.max_proposals} actions. Respond with JSON only.`;

    return message;
  }

  private parseResponse(response: Anthropic.Message): Proposal[] {
    const proposals: Proposal[] = [];

    for (const block of response.content) {
      if (block.type !== 'text') continue;

      try {
        // Try to parse as JSON
        const text = block.text.trim();
        
        // Extract JSON from markdown code blocks if present
        const jsonMatch = text.match(/```(?:json)?\s*([\s\S]*?)```/) || 
                          text.match(/(\{[\s\S]*\})/);
        
        const jsonStr = jsonMatch ? jsonMatch[1].trim() : text;
        const parsed = JSON.parse(jsonStr);

        if (parsed.proposals && Array.isArray(parsed.proposals)) {
          for (const p of parsed.proposals) {
            const proposal = this.convertProposal(p);
            if (proposal) {
              proposals.push(proposal);
            }
          }
        }
      } catch (e) {
        console.warn('Failed to parse response block:', e);
      }
    }

    return proposals;
  }

  private convertProposal(raw: any): Proposal | null {
    if (!raw || typeof raw !== 'object') return null;

    const actionKind = raw.action_kind || raw.actionKind || 'reason';
    let payload = '';
    let rationale = raw.rationale;

    // Handle tool_call for delegate actions
    if (raw.tool_call && actionKind === 'delegate') {
      const toolCall: ToolCall = {
        tool: raw.tool_call.tool,
        args: raw.tool_call.args || {},
      };
      payload = Buffer.from(JSON.stringify(toolCall)).toString('base64');
    } else if (raw.payload) {
      // Use provided payload
      payload = typeof raw.payload === 'string' 
        ? raw.payload 
        : Buffer.from(JSON.stringify(raw.payload)).toString('base64');
    }

    return {
      action_kind: this.normalizeActionKind(actionKind),
      payload,
      rationale,
    };
  }

  private normalizeActionKind(kind: string): 'reason' | 'memorize' | 'decide' | 'delegate' {
    const normalized = kind.toLowerCase();
    if (['reason', 'memorize', 'decide', 'delegate'].includes(normalized)) {
      return normalized as any;
    }
    return 'reason';
  }
}
