---
title: "ADR-0008: Use-case based MCP interface at /mcp"
type: adr
status: accepted
tags: [adr, mcp, ai, api]
created: 2026-09-27
updated: 2026-09-27
related: [mcp, backend, 0006-rest-openapi-contract-first]
---

# ADR-0008: Use-case based MCP interface at /mcp

## Context
AI agents should be able to build DR plans and support recoveries. Exposing each fine-grained REST
endpoint as a tool would give agents over 100 tools, and they would need many calls per workflow step.

## Options considered
- **One tool per REST operation:** complete, but noisy and error-prone for models.
- **Use-case tools:** one tool per workflow action, combining several operations.
- **An MCP SDK (`rmcp`)** versus implementing the small protocol subset directly.

## Decision
- Provide an MCP endpoint at **`/mcp`** with **use-case based tools** that follow the 15-step workflow and
  the recovery run (for example, `add_mitigation` = strategy + selection + runbook with steps).
- Implement the stateless Streamable HTTP transport directly (initialize, ping, tools/list, tools/call).
- Tools call the existing application use cases, so validation, gates, tenant scoping and audit are
  identical to the REST API.

## Consequences
- Agents need few, meaningful calls, and gate failures come back as tool errors they can act on.
- There is no extra dependency, but protocol features beyond tools (resources, prompts, server-initiated
  streams) would need to be added by hand, or the adapter replaced by an SDK.
- The MCP adapter lives in `backend/src/mcp/` as another inbound adapter next to the REST `api` modules.
